mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    identity::User,
    marketplace::{
        self, Command as OfferCommand, CreateDraftInput, EditOfferInput, PublishOfferInput,
        SafetyDisclosures,
    },
    reservations::{self, Command, ReserveNowInput},
    suppliers,
};
use support::*;
use uuid::Uuid;

struct OfferSetup {
    supplier_id: Uuid,
    establishment_id: Uuid,
    point_id: Uuid,
    title: &'static str,
    admitted: u64,
    price_cents: u64,
}

async fn setup_owner_with_site(
    state: &suasobra_backend::AppState,
    owner: &mut Client,
    name: &str,
) -> (Uuid, Uuid, Uuid) {
    owner.register(state, name).await;
    let supplier_id = owner.supplier(state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&state.pool)
        .await
        .unwrap();
    let establishment_id = owner.establishment(state, supplier_id).await;
    let point_id = owner.point(state, establishment_id).await;
    (supplier_id, establishment_id, point_id)
}

async fn create_published_offer(
    state: &suasobra_backend::AppState,
    owner: &Client,
    setup: OfferSetup,
) -> Uuid {
    let owner_user = owner.get_user(state).await;
    let draft_input = CreateDraftInput {
        supplier_id: setup.supplier_id,
        establishment_id: setup.establishment_id,
        pickup_point_id: setup.point_id,
        title: setup.title.into(),
        description: format!("Surplus lot: {}", setup.title),
        category: "wood_carpentry".into(),
        subcategory: None,
        unit: "pack".into(),
        admitted_quantity: setup.admitted,
        surplus_reason: "Clean offcuts from custom shelving project".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Wear gloves; untreated pine".into()),
            ..Default::default()
        },
        price_cents: Some(setup.price_cents),
    };
    let (status, val) = marketplace::execute(
        state,
        &owner_user,
        None,
        OfferCommand::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    let offer_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    let now = Utc::now();
    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(6),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(8),
        price_cents: Some(setup.price_cents),
    };
    let (status, val) = marketplace::execute(
        state,
        &owner_user,
        None,
        OfferCommand::PublishOffer(pub_input),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    offer_id
}

async fn reserve_now(
    state: &suasobra_backend::AppState,
    consumer: &Client,
    offer_id: Uuid,
    quantity: u64,
) -> Uuid {
    let user = consumer.get_user(state).await;
    let (status, val) = reservations::execute(
        state,
        &user,
        None,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    val["id"].as_str().unwrap().parse().unwrap()
}

async fn detail(
    state: &suasobra_backend::AppState,
    viewer: Option<&User>,
    reservation_id: Uuid,
) -> Result<Value, StatusCode> {
    match reservations::get_reservation_detail(&state.pool, reservation_id, viewer).await {
        Ok(v) => Ok(v),
        Err(e) => Err(e.status),
    }
}

#[tokio::test]
async fn holder_owner_and_moderator_read_detail_with_exact_location() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "detail_part").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Detail lots",
            admitted: 8,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "detail_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let holder = consumer.get_user(&db.state).await;
    let owner_user = owner.get_user(&db.state).await;
    let moderator = db.moderator().await;
    let moderator_user = moderator.get_user(&db.state).await;

    // INV-057: the confirmed holder sees immutable terms plus the exact
    // private pickup location at the permitted stage.
    let seen = detail(&db.state, Some(&holder), reservation_id)
        .await
        .unwrap();
    assert_eq!(seen["accepted_quantity"], json!(2));
    assert_eq!(seen["accepted_price_cents"], json!(500));
    assert_eq!(seen["accepted_condition"], json!("clean_offcuts"));
    assert_eq!(seen["accepted_policy_version"], json!("2026.2-free-launch"));
    assert_eq!(
        seen["pickup_point"]["address_street"],
        json!("Sample street")
    );
    assert_eq!(seen["establishment_name"], json!("Main site"));

    // The supplier owner reads their own reservation with exact location.
    let seen = detail(&db.state, Some(&owner_user), reservation_id)
        .await
        .unwrap();
    assert_eq!(seen["accepted_price_cents"], json!(500));
    assert_eq!(
        seen["pickup_point"]["address_street"],
        json!("Sample street")
    );

    // An authorized moderator reads the same accepted terms.
    let seen = detail(&db.state, Some(&moderator_user), reservation_id)
        .await
        .unwrap();
    assert_eq!(seen["id"], json!(reservation_id.to_string()));
    assert_eq!(seen["accepted_price_cents"], json!(500));
    db.finish().await;
}

#[tokio::test]
async fn outsiders_and_anonymous_cannot_read_or_list() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "detail_priv").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Private lots",
            admitted: 8,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "detail_priv_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    let mut outsider = Client::new();
    outsider.register(&db.state, "detail_outsider").await;
    let outsider_user = outsider.get_user(&db.state).await;

    // INV-057: a bare UUID grants no access; outsiders get 404 with no
    // private terms, location or existence signal beyond the code.
    assert_eq!(
        detail(&db.state, Some(&outsider_user), reservation_id).await,
        Err(StatusCode::NOT_FOUND)
    );
    assert_eq!(
        detail(&db.state, None, reservation_id).await,
        Err(StatusCode::NOT_FOUND)
    );
    assert!(
        reservations::list_consumer_reservations(
            &db.state.pool,
            consumer.user.unwrap(),
            &outsider_user
        )
        .await
        .is_err()
    );
    assert!(
        reservations::list_supplier_reservations(&db.state.pool, supplier_id, &outsider_user)
            .await
            .is_err()
    );

    // HTTP layer mirrors the same denials.
    let (status, _) = outsider
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/reservations/{reservation_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let mut anonymous = Client::new();
    let (status, _) = anonymous
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/reservations/{reservation_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = anonymous
        .call(&db.state, "GET", "/api/v1/reservations/", json!({}))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    db.finish().await;
}

#[tokio::test]
async fn displayed_terms_stay_stable_after_offer_and_point_edits() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "detail_stable").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Stable lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "detail_stable_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let holder = consumer.get_user(&db.state).await;
    let before = detail(&db.state, Some(&holder), reservation_id)
        .await
        .unwrap();

    // Prospective edits: correct the offer quantity upward and relocate the
    // pickup point (live version 1 -> 2).
    let owner_user = owner.get_user(&db.state).await;
    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        OfferCommand::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 14,
            reason: "Recounted four extra packs in storage".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "update_point",
        Some(point_id),
        None,
        json!({
            "address_street": "Changed street 999",
            "change_reason": "Relocated entrance for test",
        }),
    )
    .await
    .unwrap();

    // INV-020: the displayed accepted snapshot does not move with either edit.
    let after = detail(&db.state, Some(&holder), reservation_id)
        .await
        .unwrap();
    for key in [
        "accepted_quantity",
        "accepted_price_cents",
        "accepted_currency",
        "accepted_condition",
        "accepted_pickup_point_version",
        "accepted_pickup_starts_at",
        "accepted_pickup_ends_at",
        "accepted_policy_version",
    ] {
        assert_eq!(after[key], before[key], "snapshot field {key} moved");
    }
    assert_eq!(after["accepted_pickup_point_version"], json!(1));
    let live_version: i32 =
        sqlx::query_scalar("SELECT version FROM suasobra.pickup_points WHERE id=$1")
            .bind(point_id)
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    assert_eq!(live_version, 2);
    db.finish().await;
}

#[tokio::test]
async fn consumer_and_supplier_lists_are_scoped_to_participants() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "detail_lists").await;
    let first_offer = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "List lots A",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let second_offer = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "List lots B",
            admitted: 6,
            price_cents: 0,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "detail_lists_holder").await;
    reserve_now(&db.state, &consumer, first_offer, 1).await;
    reserve_now(&db.state, &consumer, second_offer, 1).await;
    let holder = consumer.get_user(&db.state).await;
    let owner_user = owner.get_user(&db.state).await;

    let mine =
        reservations::list_consumer_reservations(&db.state.pool, consumer.user.unwrap(), &holder)
            .await
            .unwrap();
    assert_eq!(mine.len(), 2);

    let theirs = reservations::list_supplier_reservations(&db.state.pool, supplier_id, &owner_user)
        .await
        .unwrap();
    assert_eq!(theirs.len(), 2);

    let mut outsider = Client::new();
    outsider.register(&db.state, "detail_lists_outsider").await;
    let outsider_user = outsider.get_user(&db.state).await;
    assert!(
        reservations::list_consumer_reservations(
            &db.state.pool,
            consumer.user.unwrap(),
            &outsider_user
        )
        .await
        .is_err()
    );
    assert!(
        reservations::list_supplier_reservations(&db.state.pool, supplier_id, &outsider_user)
            .await
            .is_err()
    );
    db.finish().await;
}

#[tokio::test]
async fn http_reservation_routes_serve_participants_only() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "detail_http").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "HTTP lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "detail_http_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;

    // Holder reads detail and own list over HTTP.
    let (status, val) = consumer
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/reservations/{reservation_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["accepted_price_cents"], json!(500));
    assert_eq!(
        val["pickup_point"]["address_street"],
        json!("Sample street")
    );
    let (status, val) = consumer
        .call(&db.state, "GET", "/api/v1/reservations/", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val.as_array().map(Vec::len), Some(1));

    // Supplier owner lists their offer reservations over HTTP.
    let (status, val) = owner
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/suppliers/{supplier_id}/reservations/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val.as_array().map(Vec::len), Some(1));

    // Outsiders are denied on every route.
    let mut outsider = Client::new();
    outsider.register(&db.state, "detail_http_outsider").await;
    let (status, _) = outsider
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/reservations/{reservation_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND); // The consumer-scoped list serves the caller's own reservations: an
    // outsider sees an empty list, never the holder's terms.
    let (status, val) = outsider
        .call(&db.state, "GET", "/api/v1/reservations/", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val.as_array().map(Vec::len), Some(0));
    let (status, _) = outsider
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/suppliers/{supplier_id}/reservations/"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    db.finish().await;
}

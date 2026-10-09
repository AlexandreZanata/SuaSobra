mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    marketplace::{
        self, Command as OfferCommand, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
    },
    reservations::{self, Command, ReserveNowInput},
};
use support::*;
use uuid::Uuid;

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

struct OfferSetup {
    supplier_id: Uuid,
    establishment_id: Uuid,
    point_id: Uuid,
    title: &'static str,
    admitted: u64,
    price_cents: u64,
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

async fn offer_buckets(
    state: &suasobra_backend::AppState,
    offer_id: Uuid,
) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT admitted_quantity, available_quantity, committed_quantity, fulfilled_quantity, disposed_quantity FROM suasobra.offers WHERE id=$1",
    )
    .bind(offer_id)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

async fn reserve(
    state: &suasobra_backend::AppState,
    consumer: &Client,
    offer_id: Uuid,
    quantity: u64,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = consumer.get_user(state).await;
    match reservations::execute(
        state,
        &user,
        key,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity,
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
}

#[tokio::test]
async fn reserve_confirms_immediately_with_snapshot_and_conserved_buckets() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_ok").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Reserve lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;

    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_buyer").await;
    let (status, val) = reserve(&db.state, &consumer, offer_id, 3, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    assert_eq!(val["status"], json!("confirmed"));
    assert_eq!(val["quantity"], json!(3));
    assert_eq!(val["accepted_quantity"], json!(3));
    assert_eq!(val["accepted_price_cents"], json!(500));
    assert_eq!(val["accepted_currency"], json!("BRL"));
    assert_eq!(val["accepted_condition"], json!("clean_offcuts"));
    assert_eq!(val["accepted_policy_version"], json!("2026.2-free-launch"));
    assert!(val["accepted_pickup_starts_at"].as_str().is_some());
    assert!(val["accepted_pickup_ends_at"].as_str().is_some());
    assert_eq!(
        val["consumer_id"],
        json!(consumer.user.unwrap().to_string())
    );

    // INV-001/002/004: admitted conserved, available down, committed up.
    let (admitted, available, committed, fulfilled, disposed) =
        offer_buckets(&db.state, offer_id).await;
    assert_eq!(
        (admitted, available, committed, fulfilled, disposed),
        (10, 7, 3, 0, 0)
    );

    // Audit records the confirmed reservation with quantity and goods value.
    let audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND action='reservation_confirmed' AND quantity_delta=-3 AND money_delta_cents=1500",
    )
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(audit, 1);
    db.finish().await;
}

#[tokio::test]
async fn reserve_rejects_zero_over_availability_and_unknown_offer() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_rej").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Scarce lots",
            admitted: 4,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_rej_buyer").await;

    let (status, _) = reserve(&db.state, &consumer, offer_id, 0, None).await;
    assert!(status.is_client_error(), "{status}");

    let (status, _) = reserve(&db.state, &consumer, offer_id, 5, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = reserve(&db.state, &consumer, Uuid::nil(), 1, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // INV-006: rejected requests allocate no quantity.
    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (4, 0));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    db.finish().await;
}

#[tokio::test]
async fn reserve_forbids_own_supplier_and_restricted_consumers() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_self").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Own lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;

    // INV-019: the supplier owner cannot reserve their own offer.
    let (status, _) = reserve(&db.state, &owner, offer_id, 1, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Reservation-restricted consumers are refused.
    let mut blocked = Client::new();
    blocked.register(&db.state, "reserve_blocked").await;
    sqlx::query("UPDATE suasobra.users SET restriction='reservation_restricted' WHERE id=$1")
        .bind(blocked.user.unwrap())
        .execute(&db.state.pool)
        .await
        .unwrap();
    let (status, _) = reserve(&db.state, &blocked, offer_id, 1, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (6, 0));
    db.finish().await;
}

#[tokio::test]
async fn reserve_rejects_unbookable_and_closed_offers() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_closed").await;
    let owner_user = owner.get_user(&db.state).await;

    // Draft offers are not reservable.
    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id: est_id,
        pickup_point_id: point_id,
        title: "Draft lots".into(),
        description: "Surplus lot: draft".into(),
        category: "wood_carpentry".into(),
        subcategory: None,
        unit: "pack".into(),
        admitted_quantity: 5,
        surplus_reason: "Clean offcuts from custom shelving project".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Wear gloves".into()),
            ..Default::default()
        },
        price_cents: Some(500),
    };
    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        OfferCommand::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    let draft_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_closed_buyer").await;
    let (status, _) = reserve(&db.state, &consumer, draft_id, 1, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Published offers with a closed booking window are not reservable.
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Stale lots",
            admitted: 5,
            price_cents: 500,
        },
    )
    .await;
    let past = Utc::now() - Duration::hours(2);
    sqlx::query("UPDATE suasobra.offers SET booking_starts_at=$2, booking_ends_at=$3 WHERE id=$1")
        .bind(offer_id)
        .bind(past - Duration::hours(2))
        .bind(past)
        .execute(&db.state.pool)
        .await
        .unwrap();
    let (status, _) = reserve(&db.state, &consumer, offer_id, 1, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    db.finish().await;
}

#[tokio::test]
async fn reserve_enforces_consumer_caps_and_rolls_back() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_caps").await;
    // Per-offer cap is 3 units: publish enough stock to exceed it.
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Capped lots",
            admitted: 12,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_caps_buyer").await;

    let (status, _) = reserve(&db.state, &consumer, offer_id, 3, None).await;
    assert_eq!(status, StatusCode::CREATED);
    // Second reservation would exceed the 3-unit per-offer cap: full rollback.
    let (status, _) = reserve(&db.state, &consumer, offer_id, 1, None).await;
    assert!(status.is_client_error(), "{status}");

    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (9, 3));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations WHERE status='confirmed'")
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    db.finish().await;
}

#[tokio::test]
async fn reserve_replay_returns_same_reservation_without_double_commit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_replay").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Replay lots",
            admitted: 8,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_replay_buyer").await;

    let (first_status, first) =
        reserve(&db.state, &consumer, offer_id, 2, Some("reserve-key-1")).await;
    assert_eq!(first_status, StatusCode::CREATED, "{first}");
    let (second_status, second) =
        reserve(&db.state, &consumer, offer_id, 2, Some("reserve-key-1")).await;
    assert_eq!(second_status, StatusCode::CREATED);
    assert_eq!(first["id"], second["id"]);

    // INV-061: replay creates no repeated quantity effect.
    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (6, 2));

    // Same key with different input is a conflict, not a second reservation.
    let (status, _) = reserve(&db.state, &consumer, offer_id, 1, Some("reserve-key-1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    db.finish().await;
}

#[tokio::test]
async fn accepted_snapshot_survives_offer_and_point_edits() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_snap").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Snapshot lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "reserve_snap_buyer").await;
    let (status, val) = reserve(&db.state, &consumer, offer_id, 2, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    let reservation_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    // INV-020/023: mutating the accepted snapshot directly is rejected.
    let denied = sqlx::query("UPDATE suasobra.reservations SET accepted_price_cents=1 WHERE id=$1")
        .bind(reservation_id)
        .execute(&db.state.pool)
        .await;
    assert!(denied.is_err());
    let denied =
        sqlx::query("UPDATE suasobra.reservations SET accepted_condition='intact' WHERE id=$1")
            .bind(reservation_id)
            .execute(&db.state.pool)
            .await;
    assert!(denied.is_err());
    let denied = sqlx::query("DELETE FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .execute(&db.state.pool)
        .await;
    assert!(denied.is_err());

    // Constraint guards reject invalid units, states and dangling references.
    let bad_qty = sqlx::query(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,0,0,500,'clean_offcuts',1,now(),now()+interval '1 hour','2026.2-free-launch')",
    )
    .bind(offer_id)
    .bind(supplier_id)
    .bind(est_id)
    .bind(point_id)
    .bind(consumer.user.unwrap())
    .execute(&db.state.pool)
    .await;
    assert!(bad_qty.is_err());
    let bad_state = sqlx::query(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, status, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,1,'awaiting_payment',1,500,'clean_offcuts',1,now(),now()+interval '1 hour','2026.2-free-launch')",
    )
    .bind(offer_id)
    .bind(supplier_id)
    .bind(est_id)
    .bind(point_id)
    .bind(consumer.user.unwrap())
    .execute(&db.state.pool)
    .await;
    assert!(bad_state.is_err());
    let bad_fk = sqlx::query(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,1,1,500,'clean_offcuts',1,now(),now()+interval '1 hour','2026.2-free-launch')",
    )
    .bind(Uuid::nil())
    .bind(supplier_id)
    .bind(est_id)
    .bind(point_id)
    .bind(consumer.user.unwrap())
    .execute(&db.state.pool)
    .await;
    assert!(bad_fk.is_err());

    // The stored snapshot is unchanged.
    let row: (i64, String, String) = sqlx::query_as(
        "SELECT accepted_quantity, accepted_condition, accepted_policy_version FROM suasobra.reservations WHERE id=$1",
    )
    .bind(reservation_id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 2);
    assert_eq!(row.1, "clean_offcuts");
    assert_eq!(row.2, "2026.2-free-launch");
    db.finish().await;
}

#[tokio::test]
async fn concurrent_last_unit_reservations_accept_at_most_one() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "reserve_race").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Last unit lots",
            admitted: 2,
            price_cents: 0,
        },
    )
    .await;
    let mut first_buyer = Client::new();
    first_buyer.register(&db.state, "reserve_race_a").await;
    let mut second_buyer = Client::new();
    second_buyer.register(&db.state, "reserve_race_b").await;

    // INV-005: two simultaneous attempts for the last 2 units behind one
    // row lock accept at most one commitment of 2.
    let (a, b) = tokio::join!(
        reserve(&db.state, &first_buyer, offer_id, 2, None),
        reserve(&db.state, &second_buyer, offer_id, 2, None),
    );
    let accepted =
        usize::from(a.0 == StatusCode::CREATED) + usize::from(b.0 == StatusCode::CREATED);
    assert_eq!(accepted, 1, "first={a:?} second={b:?}");

    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (0, 2));
    db.finish().await;
}

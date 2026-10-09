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

async fn reservation_count(state: &suasobra_backend::AppState) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&state.pool)
        .await
        .unwrap()
}

async fn confirm_count(state: &suasobra_backend::AppState, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND action=$1",
    )
    .bind(action)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn two_users_racing_for_final_units_yield_one_success() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_race").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Final units",
            admitted: 3,
            price_cents: 500,
        },
    )
    .await;
    let mut first_buyer = Client::new();
    first_buyer.register(&db.state, "confirm_race_a").await;
    let mut second_buyer = Client::new();
    second_buyer.register(&db.state, "confirm_race_b").await;

    // INV-005: simultaneous exact-quantity attempts behind the offer row lock
    // accept at most one commitment; the loser allocates nothing.
    let (a, b) = tokio::join!(
        reserve(&db.state, &first_buyer, offer_id, 3, None),
        reserve(&db.state, &second_buyer, offer_id, 3, None),
    );
    let succeeded =
        usize::from(a.0 == StatusCode::CREATED) + usize::from(b.0 == StatusCode::CREATED);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");
    let loser = if a.0 == StatusCode::CREATED { &b } else { &a };
    assert_eq!(loser.0, StatusCode::CONFLICT);

    // INV-001: buckets stay conserved and balanced after the race.
    let (admitted, available, committed, fulfilled, disposed) =
        offer_buckets(&db.state, offer_id).await;
    assert_eq!(
        (admitted, available, committed, fulfilled, disposed),
        (3, 0, 3, 0, 0)
    );
    assert_eq!(reservation_count(&db.state).await, 1);
    assert_eq!(confirm_count(&db.state, "reservation_confirmed").await, 1);
    db.finish().await;
}

#[tokio::test]
async fn parallel_offers_cannot_bypass_simultaneous_commitment_cap() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_cap").await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "confirm_cap_buyer").await;

    // Launch policy allows 3 simultaneous commitments: pre-fill 2, then race
    // two parallel confirms on different offers for the single remaining slot.
    for (i, title) in ["Cap filler A", "Cap filler B"].iter().enumerate() {
        let offer_id = create_published_offer(
            &db.state,
            &owner,
            OfferSetup {
                supplier_id,
                establishment_id: est_id,
                point_id,
                title,
                admitted: 5,
                price_cents: 500,
            },
        )
        .await;
        let (status, val) = reserve(&db.state, &consumer, offer_id, 1, None).await;
        assert_eq!(status, StatusCode::CREATED, "{val} filler {i}");
    }
    let first_offer = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Cap race A",
            admitted: 5,
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
            title: "Cap race B",
            admitted: 5,
            price_cents: 500,
        },
    )
    .await;

    let (a, b) = tokio::join!(
        reserve(&db.state, &consumer, first_offer, 1, None),
        reserve(&db.state, &consumer, second_offer, 1, None),
    );
    let succeeded =
        usize::from(a.0 == StatusCode::CREATED) + usize::from(b.0 == StatusCode::CREATED);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");
    assert!(a.0.is_client_error() || b.0.is_client_error());

    // Exactly 3 confirmed reservations exist; the losing offer is untouched.
    assert_eq!(reservation_count(&db.state).await, 3);
    let loser_offer = if a.0 == StatusCode::CREATED {
        second_offer
    } else {
        first_offer
    };
    let (_, available, committed, _, _) = offer_buckets(&db.state, loser_offer).await;
    assert_eq!((available, committed), (5, 0));
    db.finish().await;
}

#[tokio::test]
async fn parallel_free_offers_cannot_bypass_free_reservation_cap() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_freecap").await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "confirm_freecap_buyer").await;

    // Launch policy allows 2 simultaneous free reservations: hold 1, then race
    // two parallel free confirms for the single remaining free slot.
    let filler = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Free filler",
            admitted: 5,
            price_cents: 0,
        },
    )
    .await;
    let (status, val) = reserve(&db.state, &consumer, filler, 1, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    assert_eq!(val["accepted_price_cents"], json!(0));

    let first_offer = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Free race A",
            admitted: 5,
            price_cents: 0,
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
            title: "Free race B",
            admitted: 5,
            price_cents: 0,
        },
    )
    .await;

    let (a, b) = tokio::join!(
        reserve(&db.state, &consumer, first_offer, 1, None),
        reserve(&db.state, &consumer, second_offer, 1, None),
    );
    let succeeded =
        usize::from(a.0 == StatusCode::CREATED) + usize::from(b.0 == StatusCode::CREATED);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");
    assert_eq!(reservation_count(&db.state).await, 2);
    db.finish().await;
}

#[tokio::test]
async fn identical_retry_persists_a_single_reservation() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_retry").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Retry lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "confirm_retry_buyer").await;

    // INV-061: the identical retry returns the original reservation without
    // moving stock, auditing or persisting a second time.
    let (first_status, first) =
        reserve(&db.state, &consumer, offer_id, 2, Some("confirm-key-1")).await;
    assert_eq!(first_status, StatusCode::CREATED, "{first}");
    let (second_status, second) =
        reserve(&db.state, &consumer, offer_id, 2, Some("confirm-key-1")).await;
    assert_eq!(second_status, StatusCode::CREATED);
    assert_eq!(first["id"], second["id"]);

    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (4, 2));
    assert_eq!(reservation_count(&db.state).await, 1);
    assert_eq!(confirm_count(&db.state, "reservation_confirmed").await, 1);

    // Same key with different input is a conflict, never a second reservation.
    let (status, _) = reserve(&db.state, &consumer, offer_id, 1, Some("confirm-key-1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(reservation_count(&db.state).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn failed_confirm_rolls_back_buckets_and_writes_no_audit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_rollback").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Rollback lots",
            admitted: 4,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "confirm_rollback_buyer").await;

    // INV-006: the over-availability attempt fails atomically.
    let (status, _) = reserve(&db.state, &consumer, offer_id, 5, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (admitted, available, committed, fulfilled, disposed) =
        offer_buckets(&db.state, offer_id).await;
    assert_eq!(
        (admitted, available, committed, fulfilled, disposed),
        (4, 4, 0, 0, 0)
    );
    assert_eq!(reservation_count(&db.state).await, 0);
    assert_eq!(confirm_count(&db.state, "reservation_confirmed").await, 0);
    db.finish().await;
}

#[tokio::test]
async fn confirm_rejects_self_reservation_and_ineligible_accounts() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "confirm_perm").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Permission lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;

    // INV-019: the supplier owner cannot confirm their own offer.
    let (status, _) = reserve(&db.state, &owner, offer_id, 1, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // INV-018: suspended consumers hold no reservation authority.
    let mut suspended = Client::new();
    suspended.register(&db.state, "confirm_suspended").await;
    sqlx::query("UPDATE suasobra.users SET standing='suspended' WHERE id=$1")
        .bind(suspended.user.unwrap())
        .execute(&db.state.pool)
        .await
        .unwrap();
    let (status, _) = reserve(&db.state, &suspended, offer_id, 1, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, available, committed, _, _) = offer_buckets(&db.state, offer_id).await;
    assert_eq!((available, committed), (6, 0));
    assert_eq!(reservation_count(&db.state).await, 0);
    db.finish().await;
}

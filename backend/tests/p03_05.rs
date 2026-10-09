mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    marketplace::{
        self, Command as OfferCommand, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
    },
    reservations::{self, CancelReservationInput, Command, ReserveNowInput},
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

async fn call(
    state: &suasobra_backend::AppState,
    actor: &Client,
    command: Command,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match reservations::execute(state, &user, key, command).await {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
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

/// Craft a confirmed reservation whose accepted pickup window is fixed at an
/// explicit instant. Command-created snapshots always end in the future and
/// the snapshot columns are trigger-frozen, so past-deadline fixtures must be
/// inserted directly; the service code under test is unchanged.
struct SiteIds {
    supplier_id: Uuid,
    establishment_id: Uuid,
    point_id: Uuid,
}

async fn craft_reservation(
    state: &suasobra_backend::AppState,
    offer_id: Uuid,
    site: &SiteIds,
    consumer_id: Uuid,
    quantity: i64,
    pickup_ends_at: chrono::DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,$6,$6,500,'clean_offcuts',1,$7,$8,'2026.2-free-launch') RETURNING id",
    )
    .bind(offer_id)
    .bind(site.supplier_id)
    .bind(site.establishment_id)
    .bind(site.point_id)
    .bind(consumer_id)
    .bind(quantity)
    .bind(pickup_ends_at - Duration::hours(2))
    .bind(pickup_ends_at)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

async fn set_buckets(
    state: &suasobra_backend::AppState,
    offer_id: Uuid,
    available: i64,
    committed: i64,
    fulfilled: i64,
    disposed: i64,
) {
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity=$2, committed_quantity=$3, fulfilled_quantity=$4, disposed_quantity=$5 WHERE id=$1",
    )
    .bind(offer_id)
    .bind(available)
    .bind(committed)
    .bind(fulfilled)
    .bind(disposed)
    .execute(&state.pool)
    .await
    .unwrap();
}

async fn expiry_audit_count(state: &suasobra_backend::AppState, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE action=$1 AND actor_scope='system'",
    )
    .bind(action)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn late_reserve_is_rejected_after_booking_deadline() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "expiry_late").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Late lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "expiry_late_buyer").await;

    // The booking deadline passes while the pickup window is still open:
    // stale availability grants no new commitment even before any timer runs.
    let past = Utc::now() - Duration::minutes(5);
    sqlx::query("UPDATE suasobra.offers SET booking_ends_at=$2 WHERE id=$1")
        .bind(offer_id)
        .bind(past)
        .execute(&db.state.pool)
        .await
        .unwrap();
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 1,
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 6, 0, 0, 0));
    db.finish().await;
}

#[tokio::test]
async fn cancel_is_rejected_once_pickup_window_ends() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "expiry_cancel").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Due cancel lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let site = SiteIds {
        supplier_id,
        establishment_id: est_id,
        point_id,
    };
    let mut consumer = Client::new();
    consumer.register(&db.state, "expiry_cancel_holder").await;
    set_buckets(&db.state, offer_id, 8, 2, 0, 0).await;
    let reservation_id = craft_reservation(
        &db.state,
        offer_id,
        &site,
        consumer.user.unwrap(),
        2,
        Utc::now() - Duration::minutes(10),
    )
    .await;

    // The accepted pickup window has ended: synchronous cancellation refuses
    // even though no expiry run has happened yet — the reservation is due for
    // expiry, and cancelling would release units that must be disposed.
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::CancelReservation(CancelReservationInput {
            reservation_id: Some(reservation_id),
            reason: "Trying to cancel after the pickup window".into(),
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 2, 0, 0));
    let status: String = sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "confirmed");
    db.finish().await;
}

#[tokio::test]
async fn expire_due_transitions_and_disposes_exact_units() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "expiry_run").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Expiring lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let site = SiteIds {
        supplier_id,
        establishment_id: est_id,
        point_id,
    };
    let mut consumer = Client::new();
    consumer.register(&db.state, "expiry_run_holder").await;
    set_buckets(&db.state, offer_id, 7, 3, 0, 0).await;
    let reservation_id = craft_reservation(
        &db.state,
        offer_id,
        &site,
        consumer.user.unwrap(),
        3,
        Utc::now() - Duration::minutes(10),
    )
    .await;
    // The offer's own pickup window has ended as well.
    let past = Utc::now() - Duration::minutes(5);
    sqlx::query("UPDATE suasobra.offers SET booking_ends_at=$2, pickup_ends_at=$3 WHERE id=$1")
        .bind(offer_id)
        .bind(past - Duration::minutes(30))
        .bind(past)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let report = reservations::expire_due(&db.state.pool, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        report,
        suasobra_backend::reservations::ExpiryReport {
            expired_reservations: 1,
            expired_offers: 1,
            units_disposed: 10,
        }
    );

    // INV-001/007: admitted conserved, every remaining unit disposed exactly
    // once; the accepted snapshot itself is untouched.
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 0, 0, 0, 10));
    let row: (String, i64, i64) = sqlx::query_as(
        "SELECT status, accepted_quantity, accepted_price_cents FROM suasobra.reservations WHERE id=$1",
    )
    .bind(reservation_id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!((row.0.as_str(), row.1, row.2), ("expired", 3, 500));
    assert_eq!(
        expiry_audit_count(&db.state, "reservation_expired").await,
        1
    );
    assert_eq!(expiry_audit_count(&db.state, "offer_expired").await, 1);

    // Terminal fallout: the expired reservation cannot be cancelled and the
    // expired offer accepts no new reservation.
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::CancelReservation(CancelReservationInput {
            reservation_id: Some(reservation_id),
            reason: "Late cancellation after expiry".into(),
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 1,
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    db.finish().await;
}

#[tokio::test]
async fn expiry_boundary_is_exclusive_and_repeat_runs_are_noops() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "expiry_edge").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Boundary lots",
            admitted: 6,
            price_cents: 0,
        },
    )
    .await;
    let site = SiteIds {
        supplier_id,
        establishment_id: est_id,
        point_id,
    };
    let mut consumer = Client::new();
    consumer.register(&db.state, "expiry_edge_holder").await;
    set_buckets(&db.state, offer_id, 5, 1, 0, 0).await;
    let deadline = Utc::now() + Duration::minutes(30);
    let reservation_id = craft_reservation(
        &db.state,
        offer_id,
        &site,
        consumer.user.unwrap(),
        1,
        deadline,
    )
    .await;

    // One second before the deadline nothing is due; at the exact deadline
    // the exclusive window has ended and expiry fires.
    let early = reservations::expire_due(&db.state.pool, deadline - Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(early.expired_reservations, 0);
    assert_eq!(early.expired_offers, 0);
    let at = reservations::expire_due(&db.state.pool, deadline)
        .await
        .unwrap();
    assert_eq!(at.expired_reservations, 1);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 5, 0, 0, 1));

    // Repeatability: further runs change nothing and audit nothing new.
    let repeat = reservations::expire_due(&db.state.pool, deadline + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(repeat.expired_reservations, 0);
    assert_eq!(repeat.expired_offers, 0);
    assert_eq!(repeat.units_disposed, 0);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 5, 0, 0, 1));
    assert_eq!(
        expiry_audit_count(&db.state, "reservation_expired").await,
        1
    );
    let status: String = sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "expired");
    db.finish().await;
}

#[tokio::test]
async fn failed_expiry_batch_rolls_back_entirely() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "expiry_rollback").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Rollback lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let site = SiteIds {
        supplier_id,
        establishment_id: est_id,
        point_id,
    };
    let mut consumer = Client::new();
    consumer.register(&db.state, "expiry_rollback_holder").await;
    // A healthy due reservation plus a crash-corrupted one whose quantity
    // exceeds anything committed: the batch must fail atomically.
    set_buckets(&db.state, offer_id, 8, 2, 0, 0).await;
    let past = Utc::now() - Duration::minutes(10);
    let healthy =
        craft_reservation(&db.state, offer_id, &site, consumer.user.unwrap(), 2, past).await;
    let corrupted =
        craft_reservation(&db.state, offer_id, &site, consumer.user.unwrap(), 50, past).await;

    let result = reservations::expire_due(&db.state.pool, Utc::now()).await;
    assert!(result.is_err());

    // Crash/rollback preserves everything: both reservations still confirmed,
    // buckets conserved, no expiry audit written.
    for id in [healthy, corrupted] {
        let status: String =
            sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
                .bind(id)
                .fetch_one(&db.state.pool)
                .await
                .unwrap();
        assert_eq!(status, "confirmed");
    }
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 2, 0, 0));
    assert_eq!(
        expiry_audit_count(&db.state, "reservation_expired").await,
        0
    );
    assert_eq!(expiry_audit_count(&db.state, "offer_expired").await, 0);
    db.finish().await;
}

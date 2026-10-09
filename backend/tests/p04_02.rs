mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    fulfillment::{self, Command, CompletePickupInput, IssuePickupProofInput},
    identity,
    marketplace::{
        self, Command as OfferCommand, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
    },
    reservations::{self, ReserveNowInput},
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
        reservations::Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    val["id"].as_str().unwrap().parse().unwrap()
}

async fn issue_secret(
    state: &suasobra_backend::AppState,
    holder: &Client,
    reservation_id: Uuid,
) -> String {
    let user = holder.get_user(state).await;
    let (status, val) = fulfillment::execute(
        state,
        &user,
        None,
        Command::IssuePickupProof(IssuePickupProofInput {
            reservation_id: Some(reservation_id),
            collector_id: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    val["secret"].as_str().unwrap().to_owned()
}

async fn pickup(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    secret: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::CompletePickup(CompletePickupInput {
            reservation_id: Some(reservation_id),
            secret: secret.into(),
        }),
    )
    .await
    {
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

async fn pickup_audit_count(state: &suasobra_backend::AppState, reservation_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='pickup_completed'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn owner_completes_pickup_with_live_proof() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_ok").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Pickup lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    let (status, val) = pickup(&db.state, &owner, reservation_id, &secret, None).await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("picked_up"));

    // INV-007/009: the full accepted quantity moves committed→fulfilled once.
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 0, 3, 0));

    // One event records the handover with the known goods value — explicitly
    // not a provider-confirmed financial receipt.
    assert_eq!(pickup_audit_count(&db.state, reservation_id).await, 1);
    let fact: (i64, i64, String, String) = sqlx::query_as(
        "SELECT quantity_delta, money_delta_cents, actor_scope, reason FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='pickup_completed'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!((fact.0, fact.1), (-3, 1500));
    assert_eq!(fact.2, "supplier_owner");
    assert!(fact.3.contains("without financial receipt"));

    // The redeemed proof no longer verifies.
    assert!(
        fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, &secret)
            .await
            .is_err()
    );
    db.finish().await;
}

#[tokio::test]
async fn forgery_attempts_fail_without_state_change() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_forge").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Forge lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_forge_holder").await;
    let mut outsider = Client::new();
    outsider.register(&db.state, "pickup_forger").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // Neither the consumer (even with the valid secret), nor an outsider, nor
    // a wrong secret, nor a missing secret completes anything.
    let (status, _) = pickup(&db.state, &consumer, reservation_id, &secret, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = pickup(&db.state, &outsider, reservation_id, &secret, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = pickup(&db.state, &owner, reservation_id, "wrong-secret", None).await;
    assert!(status.is_client_error());
    let (status, _) = pickup(&db.state, &owner, reservation_id, "   ", None).await;
    assert!(status.is_client_error());
    let (status, _) = pickup(&db.state, &owner, Uuid::nil(), &secret, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 2, 0, 0));
    assert_eq!(pickup_audit_count(&db.state, reservation_id).await, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "confirmed");
    db.finish().await;
}

#[tokio::test]
async fn duplicate_pickup_yields_one_effect_and_replays_cleanly() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_dup").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Duplicate pickup lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_dup_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    let (status, first) = pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        Some("pickup-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // INV-007/061: a fresh duplicate finds terminal status and a used proof;
    // the identical replay returns the original sealed payload.
    let (status, _) = pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        Some("pickup-key-2"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, second) = pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        Some("pickup-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["id"], second["id"]);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 0, 2, 0));
    assert_eq!(pickup_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn concurrent_duplicate_handovers_yield_one_pickup() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_race").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Race pickup lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_race_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // Two simultaneous handovers behind the reservation row lock: exactly one
    // final pickup, one unit effect and one event.
    let (a, b) = tokio::join!(
        pickup(&db.state, &owner, reservation_id, &secret, None),
        pickup(&db.state, &owner, reservation_id, &secret, None),
    );
    let succeeded = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 0, 2, 0));
    assert_eq!(pickup_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn pickup_after_window_or_terminal_is_rejected() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_term").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Terminal pickup lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_term_holder").await;

    // A cancelled reservation keeps its live proof but can never be picked up.
    let cancelled = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let cancelled_secret = issue_secret(&db.state, &consumer, cancelled).await;
    let holder = consumer.get_user(&db.state).await;
    reservations::execute(
        &db.state,
        &holder,
        None,
        reservations::Command::CancelReservation(
            suasobra_backend::reservations::CancelReservationInput {
                reservation_id: Some(cancelled),
                reason: "Cancelling before the handover test".into(),
            },
        ),
    )
    .await
    .unwrap();
    let (status, _) = pickup(&db.state, &owner, cancelled, &cancelled_secret, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A past-window reservation is refused even with a live proof row.
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity=8, committed_quantity=2 WHERE id=$1",
    )
    .bind(offer_id)
    .execute(&db.state.pool)
    .await
    .unwrap();
    let past_id: Uuid = sqlx::query_scalar(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,2,2,500,'clean_offcuts',1,now()-interval '3 hours',now()-interval '1 hour','2026.2-free-launch') RETURNING id",
    )
    .bind(offer_id)
    .bind(supplier_id)
    .bind(est_id)
    .bind(point_id)
    .bind(consumer.user.unwrap())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    let live_secret = "live-secret-for-past-window";
    sqlx::query(
        "INSERT INTO suasobra.pickup_proofs (reservation_id, collector_id, scope, proof_hash, expires_at, created_by) VALUES ($1,$2,'holder',$3,now()+interval '1 hour',$2)",
    )
    .bind(past_id)
    .bind(consumer.user.unwrap())
    .bind(identity::hash(live_secret))
    .execute(&db.state.pool)
    .await
    .unwrap();
    let (status, _) = pickup(&db.state, &owner, past_id, live_secret, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 2, 0, 0));
    db.finish().await;
}

#[tokio::test]
async fn http_pickup_route_requires_owner_and_secret() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "pickup_http").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "HTTP pickup lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "pickup_http_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    let (status, val) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/pickup/"),
            json!({"secret": secret}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("picked_up"));
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 0, 2, 0));

    // Second POST is terminal; non-owners are refused on a fresh reservation.
    let (status, _) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/pickup/"),
            json!({"secret": secret}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    db.finish().await;
}

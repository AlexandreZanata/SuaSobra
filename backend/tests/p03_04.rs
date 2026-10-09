mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    identity::User,
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

async fn cancel(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    reason: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match reservations::execute(
        state,
        &user,
        key,
        Command::CancelReservation(CancelReservationInput {
            reservation_id: Some(reservation_id),
            reason: reason.into(),
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

async fn cancel_audit_count(state: &suasobra_backend::AppState, reservation_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='reservation_cancelled'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

async fn reservation_status(state: &suasobra_backend::AppState, reservation_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn holder_cancels_with_exact_release_and_audit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_ok").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Cancel lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;

    let (status, val) = cancel(
        &db.state,
        &consumer,
        reservation_id,
        "Plans changed, cannot pick up this week",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("cancelled"));
    assert_eq!(val["accepted_quantity"], json!(3));

    // INV-001/007: the exact 3 units return to available exactly once.
    let buckets = offer_buckets(&db.state, offer_id).await;
    assert_eq!(buckets, (10, 10, 0, 0, 0));
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "cancelled"
    );

    // One audit fact records the release with the canonical reason.
    assert_eq!(cancel_audit_count(&db.state, reservation_id).await, 1);
    let fact: (i64, String, String) = sqlx::query_as(
        "SELECT quantity_delta, actor_scope, reason FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='reservation_cancelled'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(fact.0, 3);
    assert_eq!(fact.1, "consumer");
    assert!(fact.2.contains("Plans changed"));

    // The cancelled reservation stays readable but no longer discloses the
    // exact private location to the holder (stage ended).
    let holder: User = consumer.get_user(&db.state).await;
    let seen = reservations::get_reservation_detail(&db.state.pool, reservation_id, Some(&holder))
        .await
        .unwrap();
    assert_eq!(seen["status"], json!("cancelled"));
    assert_eq!(
        seen["pickup_point"]["address_street"],
        json!("[Private location revealed after reservation]")
    );
    db.finish().await;
}

#[tokio::test]
async fn supplier_owner_and_moderator_cancel_with_attribution() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_supp").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Supplier cancel lots",
            admitted: 8,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_supp_holder").await;
    let first = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let second = reserve_now(&db.state, &consumer, offer_id, 1).await;

    // Supplier owner cancels the first reservation (cannot fulfill it).
    let (status, val) = cancel(
        &db.state,
        &owner,
        first,
        "Stock damaged by humidity, cannot fulfill",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(offer_buckets(&db.state, offer_id).await, (8, 7, 1, 0, 0));
    let scope: String = sqlx::query_scalar(
        "SELECT actor_scope FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='reservation_cancelled'",
    )
    .bind(first.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(scope, "supplier_owner");

    // A moderator cancels the second one under case authority.
    let moderator = db.moderator().await;
    let (status, val) = cancel(
        &db.state,
        &moderator,
        second,
        "Protective cancellation after safety report",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(offer_buckets(&db.state, offer_id).await, (8, 8, 0, 0, 0));
    db.finish().await;
}

#[tokio::test]
async fn outsider_cannot_cancel_and_state_is_untouched() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_priv").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Private cancel lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_priv_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    let mut outsider = Client::new();
    outsider.register(&db.state, "cancel_outsider").await;
    let (status, _) = cancel(
        &db.state,
        &outsider,
        reservation_id,
        "Malicious cancellation attempt",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Rejection moves no units and writes no audit.
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 2, 0, 0));
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "confirmed"
    );
    assert_eq!(cancel_audit_count(&db.state, reservation_id).await, 0);

    // Short reasons are rejected before any state change.
    let (status, _) = cancel(&db.state, &consumer, reservation_id, "no", None).await;
    assert!(status.is_client_error());
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 2, 0, 0));

    // HTTP layer: outsider 403, anonymous 401.
    let (status, _) = outsider
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/cancel/"),
            json!({"reason": "Malicious cancellation attempt"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Anonymous writes are stopped by the session/CSRF gate (403); they
    // never reach the command or move state.
    let mut anonymous = Client::new();
    let (status, _) = anonymous
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/cancel/"),
            json!({"reason": "Anonymous cancellation attempt"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    db.finish().await;
}

#[tokio::test]
async fn duplicate_cancel_and_replay_never_double_release() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_dup").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Duplicate cancel lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_dup_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    let (status, first) = cancel(
        &db.state,
        &consumer,
        reservation_id,
        "First cancellation with a clear reason",
        Some("cancel-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // INV-007/061: a fresh duplicate is a conflict, and the identical replay
    // returns the original payload without moving stock or auditing again.
    let (status, _) = cancel(
        &db.state,
        &consumer,
        reservation_id,
        "Second cancellation attempt",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, second) = cancel(
        &db.state,
        &consumer,
        reservation_id,
        "First cancellation with a clear reason",
        Some("cancel-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["id"], second["id"]);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 6, 0, 0, 0));
    assert_eq!(cancel_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn concurrent_cancels_release_exactly_once() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_race").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Race cancel lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_race_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    // Two simultaneous cancels behind the reservation row lock: exactly one
    // releases the units; the loser conflicts without duplicating audit.
    let (a, b) = tokio::join!(
        cancel(
            &db.state,
            &consumer,
            reservation_id,
            "First racing cancellation reason",
            None
        ),
        cancel(
            &db.state,
            &consumer,
            reservation_id,
            "Second racing cancellation reason",
            None
        ),
    );
    let succeeded = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 6, 0, 0, 0));
    assert_eq!(cancel_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn terminal_and_handed_over_reservations_cannot_cancel() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_term").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Terminal lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_term_holder").await;
    let handed = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let expired = reserve_now(&db.state, &consumer, offer_id, 1).await;

    // Simulate terminal states recorded by the future pickup/expiry flows:
    // handed-over units sit in fulfilled, expired ones keep their snapshot.
    sqlx::query(
        "UPDATE suasobra.offers SET committed_quantity=1, fulfilled_quantity=2 WHERE id=$1",
    )
    .bind(offer_id)
    .execute(&db.state.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE suasobra.reservations SET status='picked_up' WHERE id=$1")
        .bind(handed)
        .execute(&db.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE suasobra.reservations SET status='expired' WHERE id=$1")
        .bind(expired)
        .execute(&db.state.pool)
        .await
        .unwrap();

    // Neither handed-over nor expired reservations return stock: late
    // cancellation is a conflict with no quantity or audit effect.
    let (status, _) = cancel(
        &db.state,
        &consumer,
        handed,
        "Late cancellation after pickup",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = cancel(
        &db.state,
        &consumer,
        expired,
        "Late cancellation after expiry",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 1, 2, 0));
    assert_eq!(cancel_audit_count(&db.state, handed).await, 0);
    assert_eq!(cancel_audit_count(&db.state, expired).await, 0);
    db.finish().await;
}

#[tokio::test]
async fn http_cancel_route_serves_holder_with_idempotency() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "cancel_http").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "HTTP cancel lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "cancel_http_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    let (status, val) = consumer
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/cancel/"),
            json!({"reason": "Cancelling over HTTP with a clear reason"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("cancelled"));
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 6, 0, 0, 0));
    db.finish().await;
}

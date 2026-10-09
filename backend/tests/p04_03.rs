mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    fulfillment::{
        self, Command, CompletePartialPickupInput, CompletePickupInput, IssuePickupProofInput,
    },
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

async fn partial_pickup(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    secret: &str,
    handover_quantity: i64,
    remainder: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::CompletePartialPickup(CompletePartialPickupInput {
            reservation_id: Some(reservation_id),
            secret: secret.into(),
            handover_quantity,
            remainder: remainder.into(),
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
}

async fn full_pickup(
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

async fn partial_audit_count(state: &suasobra_backend::AppState, reservation_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='partial_pickup_completed'",
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
async fn partial_handover_releases_remainder_for_reoffer() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_rel").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Partial release lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_rel_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-001/INV-011: 2 handed over, 1 released; conservation holds
    // (10 = 8 available + 0 committed + 2 fulfilled + 0 disposed).
    let (status, val) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        2,
        "released",
        Some("partial-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("picked_up"));

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "picked_up"
    );
    assert_eq!(partial_audit_count(&db.state, reservation_id).await, 1);
    let goods_cents: i64 =
        sqlx::query_scalar("SELECT money_delta_cents FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='partial_pickup_completed'")
            .bind(reservation_id.to_string())
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    // Unit price fixed at 500; only handed-over units recorded, no refund.
    assert_eq!(goods_cents, 1000);
    db.finish().await;
}

#[tokio::test]
async fn partial_handover_disposes_unsafe_remainder() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_disp").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Partial dispose lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_disp_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-011: 1 handed over, 2 perished units disposed, never re-offered.
    let (status, val) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        1,
        "disposed",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 0, 1, 2));
    assert_eq!(partial_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn single_unit_reservation_cannot_split_but_full_pickup_still_works() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_single").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Single unit lot",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_single_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-010: one sellable unit is indivisible; partial is refused and the
    // proof stays live, so the full handover still completes afterwards.
    let (status, _) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        1,
        "released",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        0,
        "released",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        1,
        "donate",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 5, 1, 0, 0));
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "confirmed"
    );

    let (status, val) = full_pickup(&db.state, &owner, reservation_id, &secret, None).await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 5, 0, 1, 0));
    db.finish().await;
}

#[tokio::test]
async fn partial_then_duplicate_and_late_full_conserve_units() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_dup").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Partial duplicate lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_dup_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    let (status, first) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        2,
        "released",
        Some("partial-dup-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // INV-007/INV-009/INV-014: handed-over inventory never revives. A fresh
    // duplicate finds a used proof and terminal status; the identical replay
    // returns the sealed original; a late full handover is refused.
    let (status, _) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        2,
        "released",
        Some("partial-dup-key-other"),
    )
    .await;
    assert!(status.is_client_error());
    let (status, second) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        2,
        "released",
        Some("partial-dup-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["id"], second["id"]);
    let (status, _) = full_pickup(&db.state, &owner, reservation_id, &secret, None).await;
    assert!(status.is_client_error());

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
    assert_eq!(partial_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn concurrent_partial_handovers_yield_one_pickup() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_race").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Partial race lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_race_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // Two simultaneous partial handovers behind the reservation row lock:
    // exactly one final pickup, one bucket effect and one event.
    let (a, b) = tokio::join!(
        partial_pickup(
            &db.state,
            &owner,
            reservation_id,
            &secret,
            2,
            "released",
            None
        ),
        partial_pickup(
            &db.state,
            &owner,
            reservation_id,
            &secret,
            2,
            "released",
            None
        ),
    );
    let succeeded = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");

    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
    assert_eq!(partial_audit_count(&db.state, reservation_id).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn http_partial_route_serves_owner_and_rejects_bad_payload() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "partial_http").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "HTTP partial lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "partial_http_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    let (status, val) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/pickup/partial/"),
            json!({"secret": secret, "handover_quantity": 2, "remainder": "released"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], json!("picked_up"));
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));

    // Terminal: a second POST is refused.
    let (status, _) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/pickup/partial/"),
            json!({"secret": secret, "handover_quantity": 2, "remainder": "released"}),
        )
        .await;
    assert!(status.is_client_error());

    // Fresh reservation: holder and unknown remainder values are refused.
    let fresh = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let fresh_secret = issue_secret(&db.state, &consumer, fresh).await;
    let (status, _) = consumer
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{fresh}/pickup/partial/"),
            json!({"secret": fresh_secret, "handover_quantity": 2, "remainder": "released"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{fresh}/pickup/partial/"),
            json!({"secret": fresh_secret, "handover_quantity": 2, "remainder": "restock"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 5, 3, 2, 0));
    db.finish().await;
}

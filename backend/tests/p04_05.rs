mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    fulfillment::{
        self, ChallengeNoShowInput, Command, CompletePartialPickupInput, CompletePickupInput,
        IssuePickupProofInput, ReportNoShowInput, ResolveAttendanceInput,
    },
    marketplace::{
        self, Command as OfferCommand, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
    },
    reservations::{self, CancelReservationInput, ReserveNowInput},
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

async fn cancel(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match reservations::execute(
        state,
        &user,
        None,
        reservations::Command::CancelReservation(CancelReservationInput {
            reservation_id: Some(reservation_id),
            reason: "Holder changed plans before pickup".into(),
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

async fn reservation_status(state: &suasobra_backend::AppState, reservation_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&state.pool)
        .await
        .unwrap()
}

async fn fact_count(state: &suasobra_backend::AppState, reservation_id: Uuid, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action=$2",
    )
    .bind(reservation_id.to_string())
    .bind(action)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

async fn replay_count(
    state: &suasobra_backend::AppState,
    actor_id: Uuid,
    command: &str,
    key: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.command_replays WHERE actor_id=$1 AND command_name=$2 AND idempotency_key=$3",
    )
    .bind(actor_id)
    .bind(command)
    .bind(key)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

/// Past-window confirmed fixture with committed units moved, for expiry races.
async fn past_window_confirmed(
    state: &suasobra_backend::AppState,
    offer_id: Uuid,
    supplier_id: Uuid,
    establishment_id: Uuid,
    point_id: Uuid,
    consumer_id: Uuid,
    quantity: i64,
) -> Uuid {
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity=available_quantity-$2, committed_quantity=committed_quantity+$2 WHERE id=$1",
    )
    .bind(offer_id)
    .bind(quantity)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO suasobra.reservations (offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id, quantity, accepted_quantity, accepted_price_cents, accepted_condition, accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at, accepted_policy_version) VALUES ($1,$2,$3,$4,$5,$6,$6,500,'clean_offcuts',1,now()-interval '3 hours',now()-interval '1 hour','2026.2-free-launch') RETURNING id",
    )
    .bind(offer_id)
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(point_id)
    .bind(consumer_id)
    .bind(quantity)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn full_pickup_vs_holder_cancel_race_has_one_winner() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_full_cancel").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Full vs cancel lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer
        .register(&db.state, "race_full_cancel_holder")
        .await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-007/INV-061: handover and cancellation race behind the reservation
    // row lock; exactly one terminal outcome, conserved units, one fact.
    let (a, b) = tokio::join!(
        full_pickup(&db.state, &owner, reservation_id, &secret, None),
        cancel(&db.state, &consumer, reservation_id),
    );
    let wins = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(wins, 1, "pickup={a:?} cancel={b:?}");

    if a.0 == StatusCode::OK {
        assert_eq!(
            reservation_status(&db.state, reservation_id).await,
            "picked_up"
        );
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
        assert_eq!(
            fact_count(&db.state, reservation_id, "pickup_completed").await,
            1
        );
        assert_eq!(
            fact_count(&db.state, reservation_id, "reservation_cancelled").await,
            0
        );
    } else {
        assert_eq!(
            reservation_status(&db.state, reservation_id).await,
            "cancelled"
        );
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 10, 0, 0, 0));
        assert_eq!(
            fact_count(&db.state, reservation_id, "reservation_cancelled").await,
            1
        );
        assert_eq!(
            fact_count(&db.state, reservation_id, "pickup_completed").await,
            0
        );
    }
    db.finish().await;
}

#[tokio::test]
async fn partial_vs_cancel_race_conserves_units() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_partial_cancel").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Partial vs cancel lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer
        .register(&db.state, "race_partial_cancel_holder")
        .await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-001/INV-007/INV-011: partial handover and cancellation race; the
    // remainder is either terminally released or returned whole.
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
        cancel(&db.state, &consumer, reservation_id),
    );
    let wins = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(wins, 1, "partial={a:?} cancel={b:?}");

    if a.0 == StatusCode::OK {
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
        assert_eq!(
            fact_count(&db.state, reservation_id, "partial_pickup_completed").await,
            1
        );
    } else {
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 10, 0, 0, 0));
        assert_eq!(
            fact_count(&db.state, reservation_id, "reservation_cancelled").await,
            1
        );
    }
    let total_facts = fact_count(&db.state, reservation_id, "partial_pickup_completed").await
        + fact_count(&db.state, reservation_id, "reservation_cancelled").await;
    assert_eq!(total_facts, 1);
    db.finish().await;
}

#[tokio::test]
async fn full_vs_partial_same_proof_race_yields_one_handover() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_full_partial").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Full vs partial lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer
        .register(&db.state, "race_full_partial_holder")
        .await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;

    // INV-009: one reservation permits at most one final handover, full or
    // partial; the losers meet a consumed proof and terminal status.
    let (a, b) = tokio::join!(
        full_pickup(&db.state, &owner, reservation_id, &secret, None),
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
    let wins = usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK);
    assert_eq!(wins, 1, "full={a:?} partial={b:?}");

    if a.0 == StatusCode::OK {
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 0, 3, 0));
    } else {
        assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 2, 0));
    }
    let handover_facts = fact_count(&db.state, reservation_id, "pickup_completed").await
        + fact_count(&db.state, reservation_id, "partial_pickup_completed").await;
    assert_eq!(handover_facts, 1);
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "picked_up"
    );
    db.finish().await;
}

#[tokio::test]
async fn expiry_and_cancel_agree_on_past_window() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_expiry_cancel").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Expiry vs cancel lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer
        .register(&db.state, "race_expiry_cancel_holder")
        .await;
    let reservation_id = past_window_confirmed(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;

    // Past the pickup deadline the reservation is due for expiry, not
    // cancellation: concurrent expiry disposes once while the cancel is
    // refused, with no double release (INV-007).
    let cancel_fut = cancel(&db.state, &consumer, reservation_id);
    let expiry_fut = async {
        reservations::expire_due(&db.state.pool, Utc::now())
            .await
            .map(|r| r.expired_reservations)
    };
    let ((cancel_status, _), expired) = tokio::join!(cancel_fut, expiry_fut);
    assert_eq!(cancel_status, StatusCode::CONFLICT);
    assert_eq!(expired.unwrap(), 1);

    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "expired"
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    assert_eq!(
        fact_count(&db.state, reservation_id, "reservation_expired").await,
        1
    );
    assert_eq!(
        fact_count(&db.state, reservation_id, "reservation_cancelled").await,
        0
    );
    db.finish().await;
}

#[tokio::test]
async fn concurrent_duplicate_declarations_collapse_to_one() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_noshow_decl").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Declaration race lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer
        .register(&db.state, "race_noshow_decl_holder")
        .await;
    let reservation_id = past_window_confirmed(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;
    reservations::expire_due(&db.state.pool, Utc::now())
        .await
        .unwrap();

    // INV-061: simultaneous identical declarations collapse through the
    // idempotency lock to a single evidence row with one deadline.
    async fn declare(
        state: &suasobra_backend::AppState,
        actor: &Client,
        reservation_id: Uuid,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let user = actor.get_user(state).await;
        match fulfillment::execute(
            state,
            &user,
            key,
            Command::ReportNoShow(ReportNoShowInput {
                reservation_id: Some(reservation_id),
                statement: "Point staffed until close".into(),
            }),
        )
        .await
        {
            Ok(ok) => ok,
            Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
        }
    }
    let (a, b) = tokio::join!(
        declare(&db.state, &owner, reservation_id, Some("decl-race-key")),
        declare(&db.state, &owner, reservation_id, Some("decl-race-key")),
    );
    assert_eq!(a.0, StatusCode::OK, "{a:?}");
    assert_eq!(b.0, StatusCode::OK, "{b:?}");
    assert_eq!(a.1["declaration_id"], b.1["declaration_id"]);

    // Declarations under different keys still yield a single fact row: the
    // reservation row lock serializes the racers and the loser meets the
    // winner's committed declaration. A fresh expired reservation keeps this
    // race independent from the sealed one above.
    let second = past_window_confirmed(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        1,
    )
    .await;
    reservations::expire_due(&db.state.pool, Utc::now())
        .await
        .unwrap();
    let (c, d) = tokio::join!(
        declare(&db.state, &owner, second, Some("decl-race-k1")),
        declare(&db.state, &owner, second, Some("decl-race-k2")),
    );
    let wins = usize::from(c.0.is_success()) + usize::from(d.0.is_success());
    assert_eq!(wins, 1, "{c:?} {d:?}");
    assert_eq!(fact_count(&db.state, second, "no_show_declared").await, 1);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 0, 0, 3));
    db.finish().await;
}

#[tokio::test]
async fn failed_commands_leave_no_success_record() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_rollback").await;
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
    let mut consumer = Client::new();
    consumer.register(&db.state, "race_rollback_holder").await;
    let moderator = db.moderator().await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 3).await;
    let secret = issue_secret(&db.state, &consumer, reservation_id).await;
    let owner_id = owner.get_user(&db.state).await.id;
    let moderator_id = moderator.get_user(&db.state).await.id;

    // A forged proof fails mid-transaction, after the guards but before any
    // bucket, audit, or replay write: nothing persists (INV-061).
    let (status, _) = full_pickup(
        &db.state,
        &owner,
        reservation_id,
        "wrong-secret",
        Some("k-forge"),
    )
    .await;
    assert!(status.is_client_error());
    assert_eq!(
        fact_count(&db.state, reservation_id, "pickup_completed").await,
        0
    );
    assert_eq!(
        replay_count(&db.state, owner_id, "complete_pickup", "k-forge").await,
        0
    );

    // An invalid split fails before touching the proof: the live secret still
    // completes the full handover afterwards, proving no poisoned state.
    let (status, _) = partial_pickup(
        &db.state,
        &owner,
        reservation_id,
        &secret,
        3,
        "released",
        Some("k-badsplit"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        fact_count(&db.state, reservation_id, "partial_pickup_completed").await,
        0
    );
    assert_eq!(
        replay_count(&db.state, owner_id, "complete_partial_pickup", "k-badsplit").await,
        0
    );

    // Attendance failures record nothing: challenge without a declaration,
    // and a premature no-show resolution.
    let user = consumer.get_user(&db.state).await;
    let (status, _) = match fulfillment::execute(
        &db.state,
        &user,
        Some("k-nodecl"),
        Command::ChallengeNoShow(ChallengeNoShowInput {
            reservation_id: Some(reservation_id),
            statement: "Nothing to challenge".into(),
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    };
    assert_eq!(status, StatusCode::CONFLICT);
    let moderator_user = moderator.get_user(&db.state).await;
    let (status, _) = match fulfillment::execute(
        &db.state,
        &moderator_user,
        Some("k-earlyresolve"),
        Command::ResolveAttendance(ResolveAttendanceInput {
            reservation_id: Some(reservation_id),
            outcome: "no_show".into(),
            rationale: "No declaration exists".into(),
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    };
    assert!(status.is_client_error());
    assert_eq!(
        fact_count(&db.state, reservation_id, "no_show_challenged").await,
        0
    );
    assert_eq!(
        fact_count(&db.state, reservation_id, "attendance_resolved").await,
        0
    );
    assert_eq!(
        replay_count(
            &db.state,
            moderator_id,
            "resolve_attendance",
            "k-earlyresolve"
        )
        .await,
        0
    );

    // The reservation is untouched and the live proof still redeems.
    assert_eq!(
        reservation_status(&db.state, reservation_id).await,
        "confirmed"
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 3, 0, 0));
    let (status, val) = full_pickup(&db.state, &owner, reservation_id, &secret, None).await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 7, 0, 3, 0));
    db.finish().await;
}

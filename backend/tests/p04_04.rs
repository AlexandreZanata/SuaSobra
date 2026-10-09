mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    fulfillment::{self, ChallengeNoShowInput, Command, ReportNoShowInput, ResolveAttendanceInput},
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

/// Age the pickup window into the past and run real expiry: confirmed →
/// expired with committed units disposed, the UC-011 precondition.
async fn expire_reservation(
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
    let reservation_id: Uuid = sqlx::query_scalar(
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
    .unwrap();
    let report = reservations::expire_due(&state.pool, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.expired_reservations, 1);
    reservation_id
}

async fn report(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    statement: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::ReportNoShow(ReportNoShowInput {
            reservation_id: Some(reservation_id),
            statement: statement.into(),
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
}

async fn challenge(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    statement: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::ChallengeNoShow(ChallengeNoShowInput {
            reservation_id: Some(reservation_id),
            statement: statement.into(),
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
}

async fn resolve(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    outcome: &str,
    rationale: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::ResolveAttendance(ResolveAttendanceInput {
            reservation_id: Some(reservation_id),
            outcome: outcome.into(),
            rationale: rationale.into(),
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

/// Fixture an aged declaration directly: history is append-only, so a past
/// declaration is inserted once with past timestamps (like the past-window
/// pickup fixtures), never rewritten afterwards.
async fn insert_aged_declaration(
    state: &suasobra_backend::AppState,
    reservation_id: Uuid,
    declarant_id: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO suasobra.audit_facts (entity_type, entity_id, action, actor_id, actor_scope, authority_or_policy, evidence_kind, reason, payload, recognized_at, effective_at, idempotency_key) VALUES ('reservation',$1,'no_show_declared',$2,'supplier_owner','2026.2-free-launch','supplier_declaration','Supplier declares the collector did not attend the pickup window: aged fixture',jsonb_build_object('statement','aged fixture','challenge_until',now()-interval '1 hour'),now()-interval '25 hours',now()-interval '25 hours',$3) RETURNING id",
    )
    .bind(reservation_id.to_string())
    .bind(declarant_id)
    .bind(format!("noshow-declaration:{reservation_id}"))
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

/// Fixture an in-window challenge against an aged declaration, also by direct
/// insert with past timestamps inside the original 24h window.
async fn insert_aged_challenge(
    state: &suasobra_backend::AppState,
    reservation_id: Uuid,
    holder_id: Uuid,
    declaration_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO suasobra.audit_facts (entity_type, entity_id, action, actor_id, actor_scope, authority_or_policy, evidence_kind, reason, payload, recognized_at, effective_at, idempotency_key) VALUES ('reservation',$1,'no_show_challenged',$2,'holder','2026.2-free-launch','holder_statement','Collector challenges the failed-attendance declaration: aged fixture',jsonb_build_object('statement','aged fixture','declaration_id',$3,'challenge_until',now()-interval '1 hour'),now()-interval '24 hours',now()-interval '24 hours',$4)",
    )
    .bind(reservation_id.to_string())
    .bind(holder_id)
    .bind(declaration_id)
    .bind(format!("noshow-challenge:{reservation_id}"))
    .execute(&state.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn owner_declares_failed_attendance_with_exact_deadline() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_decl").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "No-show lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_decl_holder").await;
    let reservation_id = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));

    // INV-060: the declaration records who/what/when/why/policy, opens the
    // challenge window, and moves nothing.
    let (status, val) = report(
        &db.state,
        &owner,
        reservation_id,
        "Point staffed until close; collector never arrived",
        Some("noshow-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert!(val["declaration_id"].is_string());
    let until: chrono::DateTime<Utc> =
        serde_json::from_value(val["challenge_until"].clone()).unwrap();
    let (recognized, stored_until): (chrono::DateTime<Utc>, chrono::DateTime<Utc>) = sqlx::query_as(
        "SELECT recognized_at, (payload->>'challenge_until')::timestamptz FROM suasobra.audit_facts WHERE id=$1",
    )
    .bind(val["declaration_id"].as_str().unwrap().parse::<Uuid>().unwrap())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    let gap = (stored_until - recognized).num_seconds();
    assert_eq!(gap, 24 * 3600, "challenge window must be exactly 24h");
    assert!(
        (until - stored_until).num_seconds().abs() <= 1,
        "response deadline must match the stored deadline"
    );

    // Identical replay returns the sealed original; status, buckets, and
    // penalty-free history are untouched.
    let (status, second) = report(
        &db.state,
        &owner,
        reservation_id,
        "Point staffed until close; collector never arrived",
        Some("noshow-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(val["declaration_id"], second["declaration_id"]);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    assert_eq!(
        fact_count(&db.state, reservation_id, "no_show_declared").await,
        1
    );
    let status_now: String =
        sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
            .bind(reservation_id)
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    assert_eq!(status_now, "expired");
    db.finish().await;
}

#[tokio::test]
async fn declaration_refuses_live_picked_handed_and_duplicate() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_scope").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Scope lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_scope_holder").await;
    let mut outsider = Client::new();
    outsider.register(&db.state, "noshow_scope_other").await;

    // A live confirmed reservation is not a failed attendance.
    let live = reserve_now(&db.state, &consumer, offer_id, 1).await;
    let (status, _) = report(&db.state, &owner, live, "Too early", None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Outsiders cannot declare, even on an expired reservation.
    let gone = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        1,
    )
    .await;
    let (status, _) = report(&db.state, &outsider, gone, "Meddling", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = report(&db.state, &consumer, gone, "Self blame", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A second declaration changes nothing (INV-061).
    let (status, _) = report(&db.state, &owner, gone, "First", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = report(&db.state, &owner, gone, "Second", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(fact_count(&db.state, gone, "no_show_declared").await, 1);
    db.finish().await;
}

#[tokio::test]
async fn suspended_holder_keeps_challenge_access_outsiders_denied() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_chal").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Challenge lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_chal_holder").await;
    let mut outsider = Client::new();
    outsider.register(&db.state, "noshow_chal_other").await;
    let reservation_id = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;
    let (status, _) = report(&db.state, &owner, reservation_id, "Never arrived", None).await;
    assert_eq!(status, StatusCode::OK);

    // INV-062: a restricted holder retains case access and challenges in any
    // standing; outsiders (and the owner) cannot speak for the collector.
    sqlx::query("UPDATE suasobra.users SET standing='suspended' WHERE id=$1")
        .bind(consumer.user.unwrap())
        .execute(&db.state.pool)
        .await
        .unwrap();
    let (status, val) = challenge(
        &db.state,
        &consumer,
        reservation_id,
        "Bus broke down; point was closed at 19:05",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    let (status, _) = challenge(&db.state, &outsider, reservation_id, "Meddling", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = challenge(&db.state, &owner, reservation_id, "Owner voice", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // One challenge exhausts the right; evidence only, nothing moves.
    let (status, _) = challenge(&db.state, &consumer, reservation_id, "Again", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        fact_count(&db.state, reservation_id, "no_show_challenged").await,
        1
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    db.finish().await;
}

#[tokio::test]
async fn late_challenge_refused_no_show_resolves_after_window() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_late").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Late lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_late_holder").await;
    let moderator = db.moderator().await;
    let reservation_id = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;
    let owner_id = owner.get_user(&db.state).await.id;
    let declaration_id = insert_aged_declaration(&db.state, reservation_id, owner_id).await;
    insert_aged_challenge(
        &db.state,
        reservation_id,
        consumer.user.unwrap(),
        declaration_id,
    )
    .await;

    // Exact deadline: the window elapsed, so a challenge is late.
    let (status, _) = challenge(&db.state, &consumer, reservation_id, "Too late", None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The moderator resolves no-show on the unchallenged record; repeating
    // the outcome changes nothing and moves no quantity or money.
    let (status, val) = resolve(
        &db.state,
        &moderator,
        reservation_id,
        "no_show",
        "Readiness declared, no incident on file, window elapsed unchallenged",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["outcome"], json!("no_show"));
    assert_eq!(val["challenged"], json!(true));
    let (status, _) = resolve(
        &db.state,
        &moderator,
        reservation_id,
        "no_show",
        "Repeat",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // An appeal supersedes only as a linked correction with history intact.
    let (status, val) = resolve(
        &db.state,
        &moderator,
        reservation_id,
        "supplier_failure",
        "Appeal review: point logs show the shutters closed early",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert!(val["corrects"].is_string());
    let first_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='attendance_resolved' AND payload->>'outcome'='no_show'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(val["corrects"], json!(first_id.to_string()));
    let correction_of: Option<Uuid> =
        sqlx::query_scalar("SELECT correction_of FROM suasobra.audit_facts WHERE id=$1")
            .bind(
                val["resolution_id"]
                    .as_str()
                    .unwrap()
                    .parse::<Uuid>()
                    .unwrap(),
            )
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    assert_eq!(correction_of, Some(first_id));
    assert_eq!(
        fact_count(&db.state, reservation_id, "attendance_resolved").await,
        2
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    let money: Option<i64> = sqlx::query_scalar(
        "SELECT money_delta_cents FROM suasobra.audit_facts WHERE entity_type='reservation' AND entity_id=$1 AND action='attendance_resolved'",
    )
    .bind(reservation_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(money, None);
    db.finish().await;
}

#[tokio::test]
async fn early_no_show_refused_exoneration_may_resolve_and_appeal_corrects() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_appeal").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Appeal lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_appeal_holder").await;
    let moderator = db.moderator().await;
    let reservation_id = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;
    let (status, _) = report(&db.state, &owner, reservation_id, "Never arrived", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = challenge(
        &db.state,
        &consumer,
        reservation_id,
        "Point shutters were down at the stated time",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // No punishment before the consumer's opportunity: no-show inside the
    // window is refused, but exoneration may resolve on the evidence now.
    let (status, _) = resolve(
        &db.state,
        &moderator,
        reservation_id,
        "no_show",
        "Hasty attribution",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, val) = resolve(
        &db.state,
        &moderator,
        reservation_id,
        "supplier_failure",
        "Photo evidence shows the point closed early",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["challenged"], json!(true));
    assert_eq!(
        fact_count(&db.state, reservation_id, "attendance_resolved").await,
        1
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    db.finish().await;
}

#[tokio::test]
async fn http_attendance_routes_serve_roles_and_replay_sealed() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "noshow_http").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "HTTP no-show lots",
            admitted: 10,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "noshow_http_holder").await;
    let mut moderator = db.moderator().await;
    let reservation_id = expire_reservation(
        &db.state,
        offer_id,
        supplier_id,
        est_id,
        point_id,
        consumer.user.unwrap(),
        2,
    )
    .await;

    let (status, val) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/attendance/report/"),
            json!({"statement": "Collector never arrived"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert!(val["challenge_until"].is_string());

    // Holder challenges over HTTP; strangers cannot resolve.
    let (status, val) = consumer
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/attendance/challenge/"),
            json!({"statement": "I was there at 18:40"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    let (status, _) = consumer
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/attendance/resolve/"),
            json!({"outcome": "neutral", "rationale": "Self clearing"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Moderator resolves neutral inside the window; unknown outcomes are
    // rejected; buckets never move on attendance evidence.
    let (status, val) = moderator
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/attendance/resolve/"),
            json!({"outcome": "neutral", "rationale": "Conflicting statements, no corroboration either way"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["outcome"], json!("neutral"));
    let (status, _) = moderator
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/reservations/{reservation_id}/attendance/resolve/"),
            json!({"outcome": "forgiven", "rationale": "Invented outcome"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (10, 8, 0, 0, 2));
    db.finish().await;
}

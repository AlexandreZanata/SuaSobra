mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    fulfillment::{self, Command, IssuePickupProofInput},
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

async fn issue(
    state: &suasobra_backend::AppState,
    actor: &Client,
    reservation_id: Uuid,
    collector_id: Option<Uuid>,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match fulfillment::execute(
        state,
        &user,
        key,
        Command::IssuePickupProof(IssuePickupProofInput {
            reservation_id: Some(reservation_id),
            collector_id,
        }),
    )
    .await
    {
        Ok(ok) => ok,
        Err(e) => (e.status, json!({"code": e.code, "message": e.message})),
    }
}

async fn proof_count(state: &suasobra_backend::AppState) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.pickup_proofs")
        .fetch_one(&state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn holder_issues_proof_for_self_with_once_shown_secret() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_self").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Proof lots",
            admitted: 8,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 2).await;

    let (status, val) = issue(&db.state, &consumer, reservation_id, None, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    assert_eq!(val["scope"], json!("holder"));
    let secret = val["secret"].as_str().unwrap();
    assert_eq!(secret.len(), 64);

    // Only the hash rests in the database; the accepted window is the expiry.
    let row: (String, String, String) = sqlx::query_as(
        "SELECT proof_hash, scope, accepted_policy_version FROM suasobra.pickup_proofs p JOIN suasobra.reservations r ON r.id=p.reservation_id WHERE p.id=$1",
    )
    .bind(val["id"].as_str().unwrap().parse::<Uuid>().unwrap())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(row.0, identity::hash(secret));
    assert_ne!(row.0, secret);
    assert_eq!(row.1, "holder");
    assert_eq!(val["expires_at"], {
        let ends: chrono::DateTime<Utc> = sqlx::query_scalar(
            "SELECT accepted_pickup_ends_at FROM suasobra.reservations WHERE id=$1",
        )
        .bind(reservation_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
        json!(ends)
    });

    // The live secret verifies to the holder scope with handover data.
    let seen = fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, secret)
        .await
        .unwrap();
    assert_eq!(seen.scope, fulfillment::CollectorScope::Holder);
    assert_eq!(seen.collector_id, consumer.user.unwrap());
    assert_eq!(seen.quantity, 2);
    assert_eq!(seen.condition, "clean_offcuts");

    // Issuance is audited without ever persisting the secret.
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='pickup_proof' AND action='pickup_proof_issued'",
    )
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    db.finish().await;
}

#[tokio::test]
async fn holder_nominates_collector_and_verify_returns_minimal_data() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_nom").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Nominee lots",
            admitted: 8,
            price_cents: 0,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_nominator").await;
    let mut nominee = Client::new();
    nominee.register(&db.state, "proof_nominee").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;

    let (status, val) = issue(
        &db.state,
        &consumer,
        reservation_id,
        Some(nominee.user.unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    assert_eq!(val["scope"], json!("nominated"));
    let secret = val["secret"].as_str().unwrap();

    // INV-057: a valid proof reveals only the required handover data —
    // exactly these keys, no contact details or addresses.
    let seen = fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, secret)
        .await
        .unwrap();
    assert_eq!(seen.scope, fulfillment::CollectorScope::Nominated);
    assert_eq!(seen.collector_id, nominee.user.unwrap());
    let keys: Vec<String> = serde_json::to_value(&seen)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        vec![
            "collector_first_name",
            "collector_id",
            "condition",
            "proof_id",
            "quantity",
            "reservation_id",
            "scope",
        ]
    );

    // Wrong secrets and cross-reservation secrets are undifferentiated rejects.
    assert!(
        fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, "wrong-secret")
            .await
            .is_err()
    );
    let other_offer = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Other lots",
            admitted: 4,
            price_cents: 0,
        },
    )
    .await;
    let other_reservation = reserve_now(&db.state, &consumer, other_offer, 1).await;
    assert!(
        fulfillment::verify_pickup_proof(&db.state.pool, other_reservation, secret)
            .await
            .is_err()
    );
    db.finish().await;
}

#[tokio::test]
async fn issue_rejects_outsiders_owner_and_bad_nominees() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_perm").await;
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
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_perm_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;
    let mut outsider = Client::new();
    outsider.register(&db.state, "proof_outsider").await;

    // Only the holder issues: outsiders and the supplier owner are refused,
    // and no proof row or audit survives the rejection.
    let (status, _) = issue(&db.state, &outsider, reservation_id, None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = issue(&db.state, &owner, reservation_id, None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Bad nominees: the supplier itself and unknown users.
    let (status, _) = issue(
        &db.state,
        &consumer,
        reservation_id,
        Some(owner.user.unwrap()),
        None,
    )
    .await;
    assert!(status.is_client_error(), "{status}");
    let owner_user = owner.get_user(&db.state).await;
    assert_ne!(owner_user.id, consumer.user.unwrap());
    let (status, _) = issue(
        &db.state,
        &consumer,
        reservation_id,
        Some(Uuid::nil()),
        None,
    )
    .await;
    assert!(status.is_client_error(), "{status}");

    assert_eq!(proof_count(&db.state).await, 0);
    db.finish().await;
}

#[tokio::test]
async fn expired_and_used_proofs_reject_and_single_use_holds() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_reuse").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Reuse lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_reuse_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;

    // An already-expired proof row rejects on verify.
    sqlx::query(
        "INSERT INTO suasobra.pickup_proofs (reservation_id, collector_id, scope, proof_hash, expires_at, created_by) VALUES ($1,$2,'holder',$3,now()-interval '1 hour',$2)",
    )
    .bind(reservation_id)
    .bind(consumer.user.unwrap())
    .bind(identity::hash("dead-secret"))
    .execute(&db.state.pool)
    .await
    .unwrap();
    assert!(
        fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, "dead-secret")
            .await
            .is_err()
    );

    // A live proof verifies, then exactly one concurrent redemption wins and
    // every later verify rejects as reused (INV-007).
    let (status, val) = issue(&db.state, &consumer, reservation_id, None, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    let secret = val["secret"].as_str().unwrap().to_owned();
    let pool = &db.state.pool;
    let consume = |secret: String| async move {
        let mut tx = pool.begin().await.unwrap();
        let out = fulfillment::consume_pickup_proof(&mut tx, reservation_id, &secret).await;
        if out.is_ok() {
            tx.commit().await.unwrap();
        }
        out.is_ok()
    };
    let (a, b) = tokio::join!(consume(secret.clone()), consume(secret.clone()));
    assert_eq!(usize::from(a) + usize::from(b), 1);
    assert!(
        fulfillment::verify_pickup_proof(&db.state.pool, reservation_id, &secret)
            .await
            .is_err()
    );
    db.finish().await;
}

#[tokio::test]
async fn issue_replay_returns_same_sealed_response() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_replay").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Replay proof lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_replay_holder").await;
    let mut nominee = Client::new();
    nominee.register(&db.state, "proof_replay_nominee").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;

    // INV-061: the identical retry returns the sealed original (same secret),
    // minting no second proof and auditing nothing new.
    let (first_status, first) = issue(
        &db.state,
        &consumer,
        reservation_id,
        None,
        Some("proof-key-1"),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED, "{first}");
    let (second_status, second) = issue(
        &db.state,
        &consumer,
        reservation_id,
        None,
        Some("proof-key-1"),
    )
    .await;
    assert_eq!(second_status, StatusCode::CREATED);
    assert_eq!(first["id"], second["id"]);
    assert_eq!(first["secret"], second["secret"]);
    assert_eq!(proof_count(&db.state).await, 1);

    // Same key with different input is a conflict, never a second proof.
    let (status, _) = issue(
        &db.state,
        &consumer,
        reservation_id,
        Some(nominee.user.unwrap()),
        Some("proof-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(proof_count(&db.state).await, 1);
    db.finish().await;
}

#[tokio::test]
async fn proof_rows_are_append_only() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "proof_immutable").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Immutable proof lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "proof_immutable_holder").await;
    let reservation_id = reserve_now(&db.state, &consumer, offer_id, 1).await;
    let (status, val) = issue(&db.state, &consumer, reservation_id, None, None).await;
    assert_eq!(status, StatusCode::CREATED, "{val}");
    let proof_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    // Hash, scope and linkage can never be rewritten; rows are never deleted.
    let denied = sqlx::query("UPDATE suasobra.pickup_proofs SET proof_hash='x' WHERE id=$1")
        .bind(proof_id)
        .execute(&db.state.pool)
        .await;
    assert!(denied.is_err());
    let denied = sqlx::query("DELETE FROM suasobra.pickup_proofs WHERE id=$1")
        .bind(proof_id)
        .execute(&db.state.pool)
        .await;
    assert!(denied.is_err());
    db.finish().await;
}

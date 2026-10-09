mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use suasobra_backend::marketplace::{
    self, Command, CreateDraftInput, EditOfferInput, PublishOfferInput, SafetyDisclosures,
    WithdrawOfferInput,
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

async fn create_draft(
    state: &suasobra_backend::AppState,
    owner: &Client,
    supplier_id: Uuid,
    establishment_id: Uuid,
    point_id: Uuid,
    title: &str,
    admitted: u64,
) -> Uuid {
    let owner_user = owner.get_user(state).await;
    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id: point_id,
        title: title.into(),
        description: format!("Surplus lot: {title}"),
        category: "wood_carpentry".into(),
        subcategory: None,
        unit: "pack".into(),
        admitted_quantity: admitted,
        surplus_reason: "Clean offcuts from custom shelving project".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Wear gloves; untreated pine".into()),
            ..Default::default()
        },
        price_cents: Some(500),
    };
    let (status, val) =
        marketplace::execute(state, &owner_user, None, Command::CreateDraft(draft_input))
            .await
            .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{val}");
    val["id"].as_str().unwrap().parse().unwrap()
}

async fn publish_offer(state: &suasobra_backend::AppState, owner: &Client, offer_id: Uuid) {
    let owner_user = owner.get_user(state).await;
    let now = Utc::now();
    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(6),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(8),
        price_cents: Some(500),
    };
    let (status, val) =
        marketplace::execute(state, &owner_user, None, Command::PublishOffer(pub_input))
            .await
            .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
}

async fn audit_count(state: &suasobra_backend::AppState, offer_id: Uuid, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1 AND action=$2",
    )
    .bind(offer_id.to_string())
    .bind(action)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn edit_draft_quantity_up_and_down_keeps_conservation_and_audits() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "edit_up_down").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Edit lots",
        10,
    )
    .await;

    // Increase 10 -> 14: admitted and available move together.
    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 14,
            reason: "Recounted four extra packs in storage".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["admitted_quantity"], 14);
    assert_eq!(val["available_quantity"], 14);
    assert_eq!(val["committed_quantity"], 0);

    // Decrease 14 -> 11: still only uncommitted availability moves.
    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 11,
            reason: "Three packs damaged; correcting declaration".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["admitted_quantity"], 11);
    assert_eq!(val["available_quantity"], 11);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(
        offer.admitted_quantity,
        offer.available_quantity
            + offer.committed_quantity
            + offer.fulfilled_quantity
            + offer.disposed_quantity
    );

    // Audit carries actor, reason, prior and resulting available buckets (INV-012).
    let row: (i64, i64, String, Option<Uuid>) = sqlx::query_as(
        "SELECT prior_quantity, resulting_quantity, reason, actor_id FROM suasobra.audit_facts WHERE entity_id=$1 AND action='offer_availability_corrected' ORDER BY recognized_at DESC LIMIT 1",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 14);
    assert_eq!(row.1, 11);
    assert!(row.2.contains("damaged"));
    assert_eq!(row.3, Some(owner_user.id));
    assert_eq!(
        audit_count(&db.state, offer_id, "offer_availability_corrected").await,
        2
    );

    db.finish().await;
}

#[tokio::test]
async fn edit_published_offer_preserves_accepted_terms() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "edit_terms").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Terms lot",
        8,
    )
    .await;
    publish_offer(&db.state, &owner, offer_id).await;
    let before = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();

    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 10,
            reason: "Found two more packs before pickup window".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    // INV-020: accepted price, intervals, point, policy and status stay fixed.
    assert_eq!(val["price_cents"], before.price_cents);
    assert_eq!(val["payment_mode"], before.payment_mode);
    let returned_booking_end: chrono::DateTime<chrono::Utc> =
        val["booking_ends_at"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        returned_booking_end.timestamp(),
        before.booking_ends_at.unwrap().timestamp()
    );
    assert_eq!(val["pickup_point_id"], before.pickup_point_id.to_string());
    assert_eq!(val["policy_version"], before.policy_version);
    assert_eq!(val["status"], "published");
    assert_eq!(val["admitted_quantity"], 10);
    assert_eq!(val["available_quantity"], 10);

    db.finish().await;
}

#[tokio::test]
async fn edit_rejects_missing_reason_and_zero_quantity() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "edit_reason").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Reason lot",
        6,
    )
    .await;
    let audits_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.audit_facts")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();

    // INV-012: reason is mandatory.
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 8,
            reason: "  ".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // Zero admitted quantity is rejected; withdraw instead.
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 0,
            reason: "Should withdraw instead of zeroing".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // Failed edits record no event.
    let audits_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.audit_facts")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(audits_before, audits_after);
    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.admitted_quantity, 6);
    assert_eq!(offer.available_quantity, 6);

    db.finish().await;
}

#[tokio::test]
async fn edit_rejects_oversubtraction_and_committed_rewrite_without_event() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "edit_over").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Over lot",
        10,
    )
    .await;

    // Simulate two committed units held by future reservations (P03): move available -> committed.
    sqlx::query(
        "UPDATE suasobra.offers SET committed_quantity=3, available_quantity=7 WHERE id=$1",
    )
    .bind(offer_id)
    .execute(&db.state.pool)
    .await
    .unwrap();

    let audits_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.audit_facts")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();

    // INV-013: cannot reduce admitted below committed+fulfilled+disposed (3).
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 2,
            reason: "Attempt to rewrite committed stock".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // Reducing admitted by more than available would oversubtract.
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 1,
            reason: "Attempt to oversubtract availability".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    let audits_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.audit_facts")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(audits_before, audits_after);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.admitted_quantity, 10);
    assert_eq!(offer.available_quantity, 7);
    assert_eq!(offer.committed_quantity, 3);

    db.finish().await;
}

#[tokio::test]
async fn edit_forbids_outsider_and_preserves_buckets() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "edit_owner").await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Owner lot",
        9,
    )
    .await;

    let mut outsider = Client::new();
    outsider.register(&db.state, "edit_outsider").await;
    let outsider_user = outsider.get_user(&db.state).await;

    let err = marketplace::execute(
        &db.state,
        &outsider_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 20,
            reason: "Outsider tries to inflate stock".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.admitted_quantity, 9);
    assert_eq!(offer.available_quantity, 9);
    assert_eq!(
        audit_count(&db.state, offer_id, "offer_availability_corrected").await,
        0
    );

    db.finish().await;
}

#[tokio::test]
async fn withdraw_moves_only_uncommitted_availability_to_disposed() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "withdraw_ok").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Withdraw lot",
        12,
    )
    .await;
    publish_offer(&db.state, &owner, offer_id).await;

    // Hold 4 units as committed so withdrawal must leave them untouched.
    sqlx::query(
        "UPDATE suasobra.offers SET committed_quantity=4, available_quantity=8 WHERE id=$1",
    )
    .bind(offer_id)
    .execute(&db.state.pool)
    .await
    .unwrap();

    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::WithdrawOffer(WithdrawOfferInput {
            offer_id: Some(offer_id),
            reason: "Storm damaged display stock; withdrawing remainder".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], "withdrawn");
    assert_eq!(val["admitted_quantity"], 12);
    assert_eq!(val["available_quantity"], 0);
    assert_eq!(val["committed_quantity"], 4);
    assert_eq!(val["disposed_quantity"], 8);
    assert_eq!(val["price_cents"], 500);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(
        offer.admitted_quantity,
        offer.available_quantity
            + offer.committed_quantity
            + offer.fulfilled_quantity
            + offer.disposed_quantity
    );

    let row: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT prior_quantity, resulting_quantity, quantity_delta, reason FROM suasobra.audit_facts WHERE entity_id=$1 AND action='offer_withdrawn'",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(row.0, 8);
    assert_eq!(row.1, 0);
    assert_eq!(row.2, -8);
    assert!(row.3.contains("Storm"));

    db.finish().await;
}

#[tokio::test]
async fn terminal_offers_cannot_be_edited_or_withdrawn_again() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "terminal_lot").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Terminal lot",
        5,
    )
    .await;

    let (status, _) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::WithdrawOffer(WithdrawOfferInput {
            offer_id: Some(offer_id),
            reason: "Supplier closing this surplus line".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);

    // Terminal withdrawn offers cannot reopen via edit or second withdrawal.
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::EditOffer(EditOfferInput {
            offer_id: Some(offer_id),
            admitted_quantity: 9,
            reason: "Attempt to reopen withdrawn offer".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::WithdrawOffer(WithdrawOfferInput {
            offer_id: Some(offer_id),
            reason: "Duplicate withdrawal attempt recorded once".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(audit_count(&db.state, offer_id, "offer_withdrawn").await, 1);

    db.finish().await;
}

#[tokio::test]
async fn edit_and_withdraw_replay_idempotency() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "replay_lot").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Replay lot",
        7,
    )
    .await;

    let edit = EditOfferInput {
        offer_id: Some(offer_id),
        admitted_quantity: 9,
        reason: "Replayable recount of two packs".into(),
    };
    let (code_1, val_1) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("edit-replay-key-1"),
        Command::EditOffer(edit.clone()),
    )
    .await
    .unwrap();
    assert_eq!(code_1, StatusCode::OK);

    // Same key + same input returns the cached payload without a duplicate audit.
    let (code_2, val_2) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("edit-replay-key-1"),
        Command::EditOffer(edit.clone()),
    )
    .await
    .unwrap();
    assert_eq!(code_2, StatusCode::OK);
    assert_eq!(val_1, val_2);
    assert_eq!(
        audit_count(&db.state, offer_id, "offer_availability_corrected").await,
        1
    );

    // Same key with different input conflicts.
    let mut different = edit.clone();
    different.admitted_quantity = 10;
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        Some("edit-replay-key-1"),
        Command::EditOffer(different),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);

    // Withdraw replay behaves the same way.
    let withdraw = WithdrawOfferInput {
        offer_id: Some(offer_id),
        reason: "Replayable withdrawal of remaining packs".into(),
    };
    let (w1, wval1) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("withdraw-replay-key-1"),
        Command::WithdrawOffer(withdraw.clone()),
    )
    .await
    .unwrap();
    assert_eq!(w1, StatusCode::OK);
    let (w2, wval2) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("withdraw-replay-key-1"),
        Command::WithdrawOffer(withdraw.clone()),
    )
    .await
    .unwrap();
    assert_eq!(w2, StatusCode::OK);
    assert_eq!(wval1, wval2);
    assert_eq!(audit_count(&db.state, offer_id, "offer_withdrawn").await, 1);

    db.finish().await;
}

#[tokio::test]
async fn http_patch_edit_and_post_withdraw_require_owner_and_reason() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "http_lot").await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "HTTP lot",
        10,
    )
    .await;

    // Owner PATCH edit over HTTP.
    let (status, val) = owner
        .call(
            &db.state,
            "PATCH",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({"admitted_quantity": 12, "reason": "HTTP recount adds two packs"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["admitted_quantity"], 12);

    // Empty reason is rejected over HTTP.
    let (status, _) = owner
        .call(
            &db.state,
            "PATCH",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({"admitted_quantity": 13, "reason": "  "}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Outsider PATCH is forbidden and preserves buckets.
    let mut outsider = Client::new();
    outsider.register(&db.state, "http_outsider").await;
    let (status, _) = outsider
        .call(
            &db.state,
            "PATCH",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({"admitted_quantity": 15, "reason": "Outsider HTTP edit attempt"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Owner POST withdraw over HTTP.
    let (status, val) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/offers/{offer_id}/withdraw/"),
            json!({"reason": "HTTP withdrawal of display remainder"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{val}");
    assert_eq!(val["status"], "withdrawn");
    assert_eq!(val["available_quantity"], 0);

    // Outsider withdraw on a fresh offer is forbidden.
    let offer_id_2 = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "HTTP lot 2",
        4,
    )
    .await;
    let (status, _) = outsider
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/offers/{offer_id_2}/withdraw/"),
            json!({"reason": "Outsider withdrawal attempt here"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    db.finish().await;
}

#[tokio::test]
async fn concurrent_withdrawals_keep_conserved_buckets() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "race_lot").await;
    let owner_user = owner.get_user(&db.state).await;
    let offer_id = create_draft(
        &db.state,
        &owner,
        supplier_id,
        est_id,
        point_id,
        "Race lot",
        10,
    )
    .await;

    let state_a = db.state.clone();
    let state_b = db.state.clone();
    let user_a = owner_user.clone();
    let user_b = owner_user.clone();

    // Two concurrent withdrawals race for the same uncommitted availability.
    let (first, second) = tokio::join!(
        marketplace::execute(
            &state_a,
            &user_a,
            Some("race-withdraw-a"),
            Command::WithdrawOffer(WithdrawOfferInput {
                offer_id: Some(offer_id),
                reason: "First concurrent withdrawal race".into(),
            }),
        ),
        marketplace::execute(
            &state_b,
            &user_b,
            Some("race-withdraw-b"),
            Command::WithdrawOffer(WithdrawOfferInput {
                offer_id: Some(offer_id),
                reason: "Second concurrent withdrawal race".into(),
            }),
        )
    );
    let successes = [first.is_ok(), second.is_ok()]
        .into_iter()
        .filter(|ok| *ok)
        .count();
    assert_eq!(
        successes, 1,
        "competing withdrawals must leave exactly one winner"
    );
    let conflict = if first.is_err() { first } else { second }.unwrap_err();
    assert_eq!(conflict.status, StatusCode::CONFLICT);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.status, "withdrawn");
    assert_eq!(
        offer.admitted_quantity,
        offer.available_quantity
            + offer.committed_quantity
            + offer.fulfilled_quantity
            + offer.disposed_quantity
    );
    assert_eq!(offer.available_quantity, 0);
    assert_eq!(offer.disposed_quantity, 10);
    assert_eq!(audit_count(&db.state, offer_id, "offer_withdrawn").await, 1);

    db.finish().await;
}

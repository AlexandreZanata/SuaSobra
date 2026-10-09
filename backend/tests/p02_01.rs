mod support;
use axum::http::StatusCode;
use serde_json::json;
use suasobra_backend::marketplace::{self, Command, CreateSurplusInput, SurplusBuckets};
use support::*;
use uuid::Uuid;

#[tokio::test]
async fn surplus_identity_and_buckets_persist_with_uuidv7() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "surplus_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let input = CreateSurplusInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Artisanal Sourdough Bread".into(),
        description: "Fresh daily surplus from bakery morning bake".into(),
        category: "bakery_pastry".into(),
        unit: "loaf".into(),
        admitted_quantity: 10,
        available_quantity: Some(10),
        committed_quantity: Some(0),
        fulfilled_quantity: Some(0),
        disposed_quantity: Some(0),
    };

    let (status, value) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("test-surplus-create-key"),
        Command::CreateSurplus(input),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::CREATED);
    let offer_id_str = value["id"].as_str().unwrap();
    let offer_id: Uuid = offer_id_str.parse().unwrap();

    // Verify native UUIDv7 version format
    let uuid_version: i16 = sqlx::query_scalar("SELECT uuid_extract_version($1::uuid)")
        .bind(offer_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(uuid_version, 7, "Persistent offer ID must be native UUIDv7");

    // Query database row directly to verify persisted effects
    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.id, offer_id);
    assert_eq!(offer.supplier_id, supplier_id);
    assert_eq!(offer.establishment_id, establishment_id);
    assert_eq!(offer.pickup_point_id, pickup_point_id);
    assert_eq!(offer.title, "Artisanal Sourdough Bread");
    assert_eq!(
        offer.description,
        "Fresh daily surplus from bakery morning bake"
    );
    assert_eq!(offer.category, "bakery_pastry");
    assert_eq!(offer.unit, "loaf");
    assert_eq!(offer.status, "draft");
    assert_eq!(offer.admitted_quantity, 10);
    assert_eq!(offer.available_quantity, 10);
    assert_eq!(offer.committed_quantity, 0);
    assert_eq!(offer.fulfilled_quantity, 0);
    assert_eq!(offer.disposed_quantity, 0);
    assert_eq!(offer.policy_version, "2026.2-free-launch");

    // Verify buckets method
    let buckets = offer.buckets();
    assert_eq!(
        buckets,
        SurplusBuckets {
            admitted: 10,
            available: 10,
            committed: 0,
            fulfilled: 0,
            disposed: 0,
        }
    );
    buckets.validate().unwrap();

    // Verify supplier listing
    let list = marketplace::list_offers_for_supplier(&db.state.pool, supplier_id)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, offer_id);

    // Verify persisted audit facts
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);

    let (action, delta, resulting): (String, i64, i64) = sqlx::query_as(
        "SELECT action, quantity_delta, resulting_quantity FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(action, "surplus_admitted");
    assert_eq!(delta, 10);
    assert_eq!(resulting, 10);

    // Verify command replay record
    let replay_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM suasobra.command_replays WHERE command_name='create_surplus' AND actor_id=$1",
    )
    .bind(owner_user.id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(replay_count, 1);

    db.finish().await;
}

#[tokio::test]
async fn database_rejects_negative_quantity_buckets() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "neg_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    // 1. Negative admitted quantity
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Neg Admitted', 'food', 'unit', -1, -1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject negative admitted_quantity"
    );

    // 2. Negative available quantity
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, committed_quantity
        ) VALUES ($1, $2, $3, 'Neg Available', 'food', 'unit', 0, -1, 1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject negative available_quantity"
    );

    // 3. Negative committed quantity
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, committed_quantity
        ) VALUES ($1, $2, $3, 'Neg Committed', 'food', 'unit', 0, 1, -1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject negative committed_quantity"
    );

    // 4. Negative fulfilled quantity
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, fulfilled_quantity
        ) VALUES ($1, $2, $3, 'Neg Fulfilled', 'food', 'unit', 0, 1, -1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject negative fulfilled_quantity"
    );

    // 5. Negative disposed quantity
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, disposed_quantity
        ) VALUES ($1, $2, $3, 'Neg Disposed', 'food', 'unit', 0, 1, -1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject negative disposed_quantity"
    );

    // Verify 0 offers persisted
    let offer_count = count(&db.state.pool, "offers").await;
    assert_eq!(offer_count, 0);

    db.finish().await;
}

#[tokio::test]
async fn database_rejects_unbalanced_quantity_buckets() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "unbal_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    // 1. Direct raw SQL insert: sum of buckets (8 + 1 + 0 + 0 = 9) != admitted (10)
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, committed_quantity
        ) VALUES ($1, $2, $3, 'Unbalanced', 'food', 'unit', 10, 8, 1)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject unbalanced buckets (under-sum)"
    );

    // 2. Direct raw SQL insert: sum of buckets (10 + 2 = 12) != admitted (10)
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity, committed_quantity
        ) VALUES ($1, $2, $3, 'Overbalanced', 'food', 'unit', 10, 10, 2)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject unbalanced buckets (over-sum)"
    );

    // 3. Insert a valid offer, then attempt an update that violates conservation
    let valid_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Valid Initially', 'food', 'unit', 10, 10)
        RETURNING id
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();

    let update_res = sqlx::query("UPDATE suasobra.offers SET available_quantity = 5 WHERE id = $1")
        .bind(valid_id)
        .execute(&db.state.pool)
        .await;
    assert!(
        update_res.is_err(),
        "Database must reject update violating conserved inventory"
    );

    // 4. Typed Rust command rejects unbalanced buckets
    let owner_user = owner_client.get_user(&db.state).await;

    let input = CreateSurplusInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Unbalanced via Service".into(),
        description: "".into(),
        category: "food".into(),
        unit: "unit".into(),
        admitted_quantity: 10,
        available_quantity: Some(5),
        committed_quantity: Some(1),
        fulfilled_quantity: Some(0),
        disposed_quantity: Some(0),
    };

    let service_res =
        marketplace::execute(&db.state, &owner_user, None, Command::CreateSurplus(input)).await;
    assert!(
        service_res.is_err(),
        "Service must reject unbalanced buckets"
    );

    db.finish().await;
}

#[tokio::test]
async fn database_rejects_fractional_quantity_buckets() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "frac_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    // Direct SQL insert with fractional value into bigint column
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Fractional', 'food', 'unit', '10.5', '10.5')
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject fractional input syntax for bigint column"
    );

    // Rust deserialization rejects float for whole quantity
    let bad_json = json!({
        "supplier_id": supplier_id,
        "establishment_id": establishment_id,
        "pickup_point_id": pickup_point_id,
        "title": "Fractional",
        "category": "food",
        "unit": "unit",
        "admitted_quantity": 10.5
    });
    let parse_res: Result<CreateSurplusInput, _> = serde_json::from_value(bad_json);
    assert!(
        parse_res.is_err(),
        "Typed command must reject fractional JSON number for quantity"
    );

    db.finish().await;
}

#[tokio::test]
async fn database_rejects_invalid_foreign_keys() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "fk_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let bogus_uuid = Uuid::nil();

    // 1. Invalid supplier_id
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Bad Supplier', 'food', 'unit', 5, 5)
        "#,
    )
    .bind(bogus_uuid)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject invalid supplier foreign key"
    );

    // 2. Invalid establishment_id
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Bad Establishment', 'food', 'unit', 5, 5)
        "#,
    )
    .bind(supplier_id)
    .bind(bogus_uuid)
    .bind(pickup_point_id)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject invalid establishment foreign key"
    );

    // 3. Invalid pickup_point_id
    let res = sqlx::query(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Bad Point', 'food', 'unit', 5, 5)
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(bogus_uuid)
    .execute(&db.state.pool)
    .await;
    assert!(
        res.is_err(),
        "Database must reject invalid pickup_point foreign key"
    );

    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

#[tokio::test]
async fn service_rejects_mismatched_establishment_or_pickup_point() {
    let db = Database::new().await;
    let mut owner_a = Client::new();
    owner_a.register(&db.state, "owner_a").await;
    let sup_a = owner_a.supplier(&db.state).await;
    let est_a = owner_a.establishment(&db.state, sup_a).await;
    let point_a = owner_a.point(&db.state, est_a).await;

    let mut owner_b = Client::new();
    owner_b.register(&db.state, "owner_b").await;
    let sup_b = owner_b.supplier(&db.state).await;
    let est_b = owner_b.establishment(&db.state, sup_b).await;
    let point_b = owner_b.point(&db.state, est_b).await;

    let user_a = owner_a.get_user(&db.state).await;

    // 1. Establishment B does not belong to Supplier A
    let input_mismatched_est = CreateSurplusInput {
        supplier_id: sup_a,
        establishment_id: est_b,
        pickup_point_id: point_a,
        title: "Cross Supplier Est".into(),
        description: "".into(),
        category: "food".into(),
        unit: "unit".into(),
        admitted_quantity: 5,
        available_quantity: Some(5),
        committed_quantity: None,
        fulfilled_quantity: None,
        disposed_quantity: None,
    };
    let res = marketplace::execute(
        &db.state,
        &user_a,
        None,
        Command::CreateSurplus(input_mismatched_est),
    )
    .await;
    assert!(
        res.is_err(),
        "Must reject establishment belonging to different supplier"
    );

    // 2. Pickup Point B does not belong to Establishment A
    let input_mismatched_point = CreateSurplusInput {
        supplier_id: sup_a,
        establishment_id: est_a,
        pickup_point_id: point_b,
        title: "Cross Est Point".into(),
        description: "".into(),
        category: "food".into(),
        unit: "unit".into(),
        admitted_quantity: 5,
        available_quantity: Some(5),
        committed_quantity: None,
        fulfilled_quantity: None,
        disposed_quantity: None,
    };
    let res = marketplace::execute(
        &db.state,
        &user_a,
        None,
        Command::CreateSurplus(input_mismatched_point),
    )
    .await;
    assert!(
        res.is_err(),
        "Must reject pickup point belonging to different establishment"
    );

    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

#[tokio::test]
async fn permission_checks_and_unauthorized_access() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "perm_owner").await;
    let supplier_id = owner.supplier(&db.state).await;
    let establishment_id = owner.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner.point(&db.state, establishment_id).await;

    // Outsider user
    let mut outsider = Client::new();
    outsider.register(&db.state, "outsider").await;
    let outsider_user = outsider.get_user(&db.state).await;

    let input = CreateSurplusInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Outsider Surplus".into(),
        description: "".into(),
        category: "food".into(),
        unit: "unit".into(),
        admitted_quantity: 5,
        available_quantity: Some(5),
        committed_quantity: None,
        fulfilled_quantity: None,
        disposed_quantity: None,
    };

    let err = marketplace::execute(
        &db.state,
        &outsider_user,
        None,
        Command::CreateSurplus(input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "permission_denied");

    // Close supplier and verify owner cannot create surplus
    let (close_status, _) = owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/suppliers/{supplier_id}/close/"),
            json!({"reason": "Business closed down"}),
        )
        .await;
    assert_eq!(close_status, StatusCode::OK);

    let owner_user = owner.get_user(&db.state).await;
    let closed_err =
        marketplace::execute(&db.state, &owner_user, None, Command::CreateSurplus(input))
            .await
            .unwrap_err();
    assert_eq!(closed_err.code, "validation_error");
    assert!(closed_err.message.contains("closed"));

    db.finish().await;
}

#[tokio::test]
async fn command_replay_idempotency_and_conflict() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "replay_owner").await;
    let supplier_id = owner.supplier(&db.state).await;
    let establishment_id = owner.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner.point(&db.state, establishment_id).await;

    let owner_user = owner.get_user(&db.state).await;

    let input = CreateSurplusInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Replay Surplus".into(),
        description: "Testing idempotent replay".into(),
        category: "produce".into(),
        unit: "kg".into(),
        admitted_quantity: 20,
        available_quantity: Some(20),
        committed_quantity: None,
        fulfilled_quantity: None,
        disposed_quantity: None,
    };

    let key = "idemp-key-surplus-001";

    // First call -> 201 Created
    let (status1, val1) = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateSurplus(input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status1, StatusCode::CREATED);
    let id1 = val1["id"].as_str().unwrap();

    // Second call with same key and same input -> 201 Created with cached response
    let (status2, val2) = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateSurplus(input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status2, StatusCode::CREATED);
    assert_eq!(val2["id"].as_str().unwrap(), id1);

    // Database still has exactly 1 offer
    assert_eq!(count(&db.state.pool, "offers").await, 1);

    // Third call with same key but different title -> 409 Conflict
    let mut changed_input = input;
    changed_input.title = "Modified Title".into();
    let conflict_err = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateSurplus(changed_input),
    )
    .await
    .unwrap_err();
    assert_eq!(conflict_err.status, StatusCode::CONFLICT);
    assert_eq!(conflict_err.code, "conflict");

    db.finish().await;
}

#[tokio::test]
async fn transaction_rollback_preserves_database_state() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "rollback_owner").await;
    let supplier_id = owner.supplier(&db.state).await;
    let establishment_id = owner.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner.point(&db.state, establishment_id).await;

    // Simulate an aborted transaction
    let mut tx = db.state.pool.begin().await.unwrap();
    let _offer_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO suasobra.offers (
            supplier_id, establishment_id, pickup_point_id,
            title, category, unit, admitted_quantity, available_quantity
        ) VALUES ($1, $2, $3, 'Rollback Me', 'food', 'unit', 10, 10)
        RETURNING id
        "#,
    )
    .bind(supplier_id)
    .bind(establishment_id)
    .bind(pickup_point_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    // Explicit rollback
    tx.rollback().await.unwrap();

    // Database must have 0 offers
    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

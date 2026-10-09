mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use suasobra_backend::marketplace::{self, Command, CreateDraftInput, SafetyDisclosures};
use support::*;
use uuid::Uuid;

#[tokio::test]
async fn valid_draft_with_substantiated_surplus_and_safety_disclosures_persists() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "draft_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;
    let future_expiry = Utc::now() + Duration::hours(24);

    let input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Artisanal Brioche Buns".into(),
        description: "Fresh morning batch surplus".into(),
        category: "food".into(),
        subcategory: Some("bakery".into()),
        unit: "pack".into(),
        admitted_quantity: 8,
        surplus_reason: "Surplus morning production exceeding counter forecast".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Keep in cool dry place, refrigerate after opening".into()),
            age_or_preparation: Some("Baked fresh today at 06:30".into()),
            expiry_or_best_before: Some(future_expiry),
            allergens_or_dietary: Some("Contains wheat, eggs, milk. Vegetarian.".into()),
            safe_handling: None,
            condition_notes: Some("Freshly packaged in bakery bags".into()),
            packaging_integrity: Some("Sealed food-grade bag".into()),
        },
        price_cents: Some(500),
    };

    let (status, value) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("draft-idemp-001"),
        Command::CreateDraft(input),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::CREATED);
    let offer_id: Uuid = value["id"].as_str().unwrap().parse().unwrap();

    // Verify native UUIDv7 version format
    let uuid_version: i16 = sqlx::query_scalar("SELECT uuid_extract_version($1::uuid)")
        .bind(offer_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(uuid_version, 7, "Offer ID must be native UUIDv7");

    // Query database row directly
    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.id, offer_id);
    assert_eq!(offer.status, "draft");
    assert_eq!(offer.category, "food");
    assert_eq!(offer.unit, "pack");
    assert_eq!(offer.admitted_quantity, 8);
    assert_eq!(offer.available_quantity, 8);
    assert_eq!(offer.committed_quantity, 0);
    assert_eq!(offer.fulfilled_quantity, 0);
    assert_eq!(offer.disposed_quantity, 0);
    assert_eq!(
        offer.surplus_reason,
        "Surplus morning production exceeding counter forecast"
    );
    assert_eq!(offer.condition, "prepared_today");
    assert_eq!(offer.price_cents, 500);
    assert_eq!(offer.currency, "BRL");

    // Verify persisted safety disclosures in JSONB
    assert_eq!(
        offer.safety_disclosures["storage_instructions"],
        "Keep in cool dry place, refrigerate after opening"
    );
    assert_eq!(
        offer.safety_disclosures["age_or_preparation"],
        "Baked fresh today at 06:30"
    );
    assert_eq!(
        offer.safety_disclosures["allergens_or_dietary"],
        "Contains wheat, eggs, milk. Vegetarian."
    );

    // Verify audit facts
    let (action, delta, resulting): (String, i64, i64) = sqlx::query_as(
        "SELECT action, quantity_delta, resulting_quantity FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(action, "draft_created");
    assert_eq!(delta, 8);
    assert_eq!(resulting, 8);

    db.finish().await;
}

#[tokio::test]
async fn non_food_category_draft_with_valid_disclosures_persists() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "produce_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Organic Red Apples".into(),
        description: "Surface cosmetic blemishes".into(),
        category: "fruit_vegetables".into(),
        subcategory: Some("fresh_produce".into()),
        unit: "kg".into(),
        admitted_quantity: 15,
        surplus_reason: "Surface cosmetic defects, fully edible fresh harvest".into(),
        condition: "cosmetic_defect".into(),
        safety_disclosures: SafetyDisclosures {
            condition_notes: Some("Light hail marks on skin, firm sweet flesh".into()),
            safe_handling: Some("Wash before consuming".into()),
            packaging_integrity: Some("Packed in recyclable cardboard crate".into()),
            ..Default::default()
        },
        price_cents: Some(300),
    };

    let (status, value) =
        marketplace::execute(&db.state, &owner_user, None, Command::CreateDraft(input))
            .await
            .unwrap();

    assert_eq!(status, StatusCode::CREATED);
    let offer_id: Uuid = value["id"].as_str().unwrap().parse().unwrap();

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.category, "fruit_vegetables");
    assert_eq!(offer.condition, "cosmetic_defect");
    assert_eq!(offer.admitted_quantity, 15);
    assert_eq!(
        offer.safety_disclosures["condition_notes"],
        "Light hail marks on skin, firm sweet flesh"
    );

    db.finish().await;
}

#[tokio::test]
async fn rejects_unsupported_or_invalid_category() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "cat_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let bad_categories = ["electronics", "pharmaceuticals", "weapons", "unknown", ""];
    for bad_cat in bad_categories {
        let input = CreateDraftInput {
            supplier_id,
            establishment_id,
            pickup_point_id,
            title: "Item with invalid category".into(),
            description: "".into(),
            category: bad_cat.into(),
            subcategory: None,
            unit: "unit".into(),
            admitted_quantity: 5,
            surplus_reason: "Overstock inventory from warehouse".into(),
            condition: "fresh".into(),
            safety_disclosures: SafetyDisclosures::default(),
            price_cents: None,
        };

        let err = marketplace::execute(&db.state, &owner_user, None, Command::CreateDraft(input))
            .await
            .unwrap_err();
        assert_eq!(
            err.code, "validation_error",
            "Category '{bad_cat}' must be rejected"
        );
    }

    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

#[tokio::test]
async fn rejects_empty_or_unsubstantiated_surplus_classification() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "class_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let invalid_reasons = ["", "   ", "abc", "1234"];
    for reason in invalid_reasons {
        let input = CreateDraftInput {
            supplier_id,
            establishment_id,
            pickup_point_id,
            title: "Genuine Surplus Item".into(),
            description: "".into(),
            category: "packaging".into(),
            subcategory: None,
            unit: "box".into(),
            admitted_quantity: 10,
            surplus_reason: reason.into(),
            condition: "clean_offcuts".into(),
            safety_disclosures: SafetyDisclosures::default(),
            price_cents: None,
        };

        let err = marketplace::execute(&db.state, &owner_user, None, Command::CreateDraft(input))
            .await
            .unwrap_err();
        assert_eq!(
            err.code, "validation_error",
            "Empty/unsubstantiated reason '{reason}' must be rejected"
        );
    }

    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

#[tokio::test]
async fn zero_price_does_not_exempt_safety_and_condition_checks() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "zero_price_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    // Zero-priced food offer with missing safety disclosure (missing storage instructions)
    let missing_storage = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Free Donated Sandwiches".into(),
        description: "Zero price donation".into(),
        category: "food".into(),
        subcategory: None,
        unit: "unit".into(),
        admitted_quantity: 4,
        surplus_reason: "Daily surplus from corporate event".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: None, // Missing!
            age_or_preparation: Some("Made today at 11:00".into()),
            expiry_or_best_before: Some(Utc::now() + Duration::hours(6)),
            allergens_or_dietary: Some("Contains tuna, mayonnaise, egg".into()),
            ..Default::default()
        },
        price_cents: Some(0), // Free! INV-017 requires full safety checks.
    };

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(missing_storage),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("storage_instructions"));

    // Zero-priced food offer with complete safety disclosures succeeds
    let valid_free = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Free Donated Sandwiches".into(),
        description: "Zero price donation with complete disclosures".into(),
        category: "food".into(),
        subcategory: None,
        unit: "unit".into(),
        admitted_quantity: 4,
        surplus_reason: "Daily surplus from corporate event".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Refrigerate under 5°C immediately".into()),
            age_or_preparation: Some("Made today at 11:00".into()),
            expiry_or_best_before: Some(Utc::now() + Duration::hours(6)),
            allergens_or_dietary: Some("Contains tuna, mayonnaise, egg".into()),
            ..Default::default()
        },
        price_cents: Some(0),
    };

    let (status, val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(valid_free),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(val["price_cents"], 0);

    db.finish().await;
}

#[tokio::test]
async fn food_disclosures_cannot_conceal_age_expiry_storage_or_allergens() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "conceal_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let base_valid = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Fresh Meal Box".into(),
        description: "".into(),
        category: "food".into(),
        subcategory: None,
        unit: "box".into(),
        admitted_quantity: 3,
        surplus_reason: "Prepared surplus from cafeteria lunch".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Keep hot or refrigerate".into()),
            age_or_preparation: Some("Prepared today at 12:00".into()),
            expiry_or_best_before: Some(Utc::now() + Duration::hours(8)),
            allergens_or_dietary: Some("Contains peanuts".into()),
            ..Default::default()
        },
        price_cents: Some(800),
    };

    // 1. Missing age_or_preparation
    let mut no_age = base_valid.clone();
    no_age.safety_disclosures.age_or_preparation = None;
    let err = marketplace::execute(&db.state, &owner_user, None, Command::CreateDraft(no_age))
        .await
        .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("age_or_preparation"));

    // 2. Missing allergens_or_dietary
    let mut no_allergens = base_valid.clone();
    no_allergens.safety_disclosures.allergens_or_dietary = None;
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(no_allergens),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("allergens_or_dietary"));

    // 3. Missing expiry_or_best_before
    let mut no_expiry = base_valid.clone();
    no_expiry.safety_disclosures.expiry_or_best_before = None;
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(no_expiry),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("expiry_or_best_before"));

    // 4. Past expiry (expired food)
    let mut past_expiry = base_valid.clone();
    past_expiry.safety_disclosures.expiry_or_best_before = Some(Utc::now() - Duration::hours(2));
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(past_expiry),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("Expired food cannot be offered"));

    // 5. Unsafe / prohibited food condition
    let mut spoiled_food = base_valid.clone();
    spoiled_food.condition = "spoiled".into();
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(spoiled_food),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("Food condition must be one of"));

    assert_eq!(count(&db.state.pool, "offers").await, 0);

    db.finish().await;
}

#[tokio::test]
async fn rejects_unknown_fields_and_invented_normal_prices() {
    let bogus_uuid = Uuid::nil();

    // 1. Rejects invented normal price
    let invented_price_json = json!({
        "supplier_id": bogus_uuid,
        "establishment_id": bogus_uuid,
        "pickup_point_id": bogus_uuid,
        "title": "Item with fake original price",
        "category": "food",
        "unit": "pack",
        "admitted_quantity": 5,
        "surplus_reason": "Excess inventory",
        "condition": "prepared_today",
        "safety_disclosures": {},
        "invented_normal_price": 5000
    });
    let parse_res: Result<CreateDraftInput, _> = serde_json::from_value(invented_price_json);
    assert!(
        parse_res.is_err(),
        "Must reject invented_normal_price field"
    );

    // 2. Rejects original_price field
    let original_price_json = json!({
        "supplier_id": bogus_uuid,
        "establishment_id": bogus_uuid,
        "pickup_point_id": bogus_uuid,
        "title": "Item with fake discount",
        "category": "food",
        "unit": "pack",
        "admitted_quantity": 5,
        "surplus_reason": "Excess inventory",
        "condition": "prepared_today",
        "safety_disclosures": {},
        "original_price": 10000
    });
    let parse_res: Result<CreateDraftInput, _> = serde_json::from_value(original_price_json);
    assert!(parse_res.is_err(), "Must reject original_price field");

    // 3. Rejects arbitrary unknown field
    let unknown_field_json = json!({
        "supplier_id": bogus_uuid,
        "establishment_id": bogus_uuid,
        "pickup_point_id": bogus_uuid,
        "title": "Item with unknown field",
        "category": "food",
        "unit": "pack",
        "admitted_quantity": 5,
        "surplus_reason": "Excess inventory",
        "condition": "prepared_today",
        "safety_disclosures": {},
        "unsupported_flag": true
    });
    let parse_res: Result<CreateDraftInput, _> = serde_json::from_value(unknown_field_json);
    assert!(parse_res.is_err(), "Must reject unknown fields");
}

#[tokio::test]
async fn permission_and_supplier_checks_for_draft() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "draft_perm_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    // Outsider user
    let mut outsider = Client::new();
    outsider.register(&db.state, "draft_outsider").await;
    let outsider_user = outsider.get_user(&db.state).await;

    let input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Outsider Draft".into(),
        description: "".into(),
        category: "wood_carpentry".into(),
        subcategory: None,
        unit: "piece".into(),
        admitted_quantity: 5,
        surplus_reason: "Clean carpenter offcuts from table manufacturing".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures::default(),
        price_cents: None,
    };

    let err = marketplace::execute(
        &db.state,
        &outsider_user,
        None,
        Command::CreateDraft(input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "permission_denied");

    // Close supplier and verify owner cannot create draft
    let (close_status, _) = owner_client
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/suppliers/{supplier_id}/close/"),
            json!({"reason": "Business closed down"}),
        )
        .await;
    assert_eq!(close_status, StatusCode::OK);

    let owner_user = owner_client.get_user(&db.state).await;
    let closed_err =
        marketplace::execute(&db.state, &owner_user, None, Command::CreateDraft(input))
            .await
            .unwrap_err();
    assert_eq!(closed_err.code, "validation_error");
    assert!(closed_err.message.contains("closed"));

    db.finish().await;
}

#[tokio::test]
async fn command_replay_and_rollback_for_draft() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "draft_replay_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let owner_user = owner_client.get_user(&db.state).await;

    let input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Replay Cotton Offcuts".into(),
        description: "Surplus fabric offcuts".into(),
        category: "textiles_offcuts".into(),
        subcategory: None,
        unit: "kg".into(),
        admitted_quantity: 10,
        surplus_reason: "Clean fabric cutting remnants from clothing atelier".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures {
            condition_notes: Some("100% natural cotton remnants".into()),
            ..Default::default()
        },
        price_cents: Some(1500),
    };

    let key = "draft-replay-key-001";

    // 1. Initial execution
    let (status1, val1) = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateDraft(input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status1, StatusCode::CREATED);
    let id1 = val1["id"].as_str().unwrap();

    // 2. Replay with identical input returns cached response
    let (status2, val2) = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateDraft(input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status2, StatusCode::CREATED);
    assert_eq!(val2["id"].as_str().unwrap(), id1);
    assert_eq!(count(&db.state.pool, "offers").await, 1);

    // 3. Replay with conflicting input returns 409 Conflict
    let mut modified = input;
    modified.title = "Modified Cotton Remnants".into();
    let conflict_err = marketplace::execute(
        &db.state,
        &owner_user,
        Some(key),
        Command::CreateDraft(modified),
    )
    .await
    .unwrap_err();
    assert_eq!(conflict_err.status, StatusCode::CONFLICT);
    assert_eq!(conflict_err.code, "conflict");

    db.finish().await;
}

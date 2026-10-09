mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use suasobra_backend::marketplace::{
    self, Command, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
};
use support::*;
use uuid::Uuid;

#[tokio::test]
async fn valid_fixed_price_offer_publication_succeeds_and_persists_intervals_and_audit() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "pub_owner_1").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();
    let expiry = now + Duration::hours(48);

    // Create draft
    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Artisanal Sourdough Bread".into(),
        description: "Fresh daily surplus loaves".into(),
        category: "food".into(),
        subcategory: Some("bakery".into()),
        unit: "loaf".into(),
        admitted_quantity: 10,
        surplus_reason: "Daily bake surplus remaining after morning peak".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Store at room temperature in paper bag".into()),
            age_or_preparation: Some("Baked at 05:00 today".into()),
            expiry_or_best_before: Some(expiry),
            allergens_or_dietary: Some("Contains gluten. Vegan.".into()),
            safe_handling: None,
            condition_notes: Some("Crispy crust, soft crumb".into()),
            packaging_integrity: Some("Paper bag intact".into()),
        },
        price_cents: Some(1200),
    };

    let (draft_status, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    assert_eq!(draft_status, StatusCode::CREATED);
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    // Publish offer
    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(8),
        pickup_starts_at: now + Duration::hours(2),
        pickup_ends_at: now + Duration::hours(10),
        price_cents: Some(1000), // Updated discounted price
    };

    let (pub_status, pub_val) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("pub-idemp-001"),
        Command::PublishOffer(pub_input),
    )
    .await
    .unwrap();

    assert_eq!(pub_status, StatusCode::OK);
    assert_eq!(pub_val["status"], "published");
    assert_eq!(pub_val["payment_mode"], "at_pickup");
    assert_eq!(pub_val["price_cents"], 1000);
    assert_eq!(pub_val["currency"], "BRL");
    assert_eq!(pub_val["available_quantity"], 10);
    assert_eq!(pub_val["admitted_quantity"], 10);

    // Verify database record directly
    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.status, "published");
    assert_eq!(offer.payment_mode, "at_pickup");
    assert_eq!(offer.price_cents, 1000);
    assert!(offer.booking_starts_at.is_some());
    assert!(offer.booking_ends_at.is_some());
    assert!(offer.pickup_starts_at.is_some());
    assert!(offer.pickup_ends_at.is_some());

    // Verify audit fact
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1 AND action='offer_published'",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);

    db.finish().await;
}

#[tokio::test]
async fn valid_zero_price_free_offer_publication_succeeds_with_free_payment_mode() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "pub_free_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();
    let expiry = now + Duration::hours(36);

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Community Surplus Bananas".into(),
        description: "Ripe bananas for donation/free collection".into(),
        category: "food".into(),
        subcategory: Some("produce".into()),
        unit: "kg".into(),
        admitted_quantity: 5,
        surplus_reason: "Surplus ripe produce suitable for immediate consumption".into(),
        condition: "near_expiry".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Keep cool".into()),
            age_or_preparation: Some("Harvested 5 days ago".into()),
            expiry_or_best_before: Some(expiry),
            allergens_or_dietary: Some("None. Vegan.".into()),
            safe_handling: None,
            condition_notes: Some("Ripe with sugar spots".into()),
            packaging_integrity: Some("Crate".into()),
        },
        price_cents: Some(0),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(6),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(8),
        price_cents: Some(0),
    };

    let (pub_status, pub_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input),
    )
    .await
    .unwrap();

    assert_eq!(pub_status, StatusCode::OK);
    assert_eq!(pub_val["status"], "published");
    assert_eq!(pub_val["payment_mode"], "free");
    assert_eq!(pub_val["price_cents"], 0);

    let offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer.payment_mode, "free");
    assert_eq!(offer.price_cents, 0);

    db.finish().await;
}

#[tokio::test]
async fn scheduled_future_offer_publication_sets_status_scheduled() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "sched_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();
    let future_start = now + Duration::hours(4);
    let future_end = now + Duration::hours(8);
    let pickup_start = now + Duration::hours(5);
    let pickup_end = now + Duration::hours(10);
    let expiry = now + Duration::hours(48);

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Evening Surprise Assortment".into(),
        description: "Scheduled evening bread bag".into(),
        category: "food".into(),
        subcategory: None,
        unit: "bag".into(),
        admitted_quantity: 4,
        surplus_reason: "Expected evening counter assortment surplus".into(),
        condition: "surprise_assortment".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Keep room temperature".into()),
            age_or_preparation: Some("Prepared today".into()),
            expiry_or_best_before: Some(expiry),
            allergens_or_dietary: Some("May contain nuts, gluten, dairy".into()),
            safe_handling: None,
            condition_notes: Some("Fresh assorted bakery products".into()),
            packaging_integrity: Some("Sealed paper bag".into()),
        },
        price_cents: Some(800),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(future_start),
        booking_ends_at: future_end,
        pickup_starts_at: pickup_start,
        pickup_ends_at: pickup_end,
        price_cents: Some(800),
    };

    let (pub_status, pub_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input),
    )
    .await
    .unwrap();

    assert_eq!(pub_status, StatusCode::OK);
    assert_eq!(pub_val["status"], "scheduled");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE entity_type='offer' AND entity_id=$1 AND action='offer_scheduled'",
    )
    .bind(offer_id.to_string())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);

    db.finish().await;
}

#[tokio::test]
async fn supplier_standing_and_publication_authority_enforced() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "standing_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Test Wood Cutoffs".into(),
        description: "Carpenter workshop cutoffs".into(),
        category: "wood_carpentry".into(),
        subcategory: None,
        unit: "pack".into(),
        admitted_quantity: 6,
        surplus_reason: "Clean offcuts from custom shelving project".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Wear gloves; untreated pine wood".into()),
            ..Default::default()
        },
        price_cents: Some(2500),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(4),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(6),
        price_cents: Some(2500),
    };

    // 1. Outsider without authority receives 403 (INV-018)
    let mut outsider_client = Client::new();
    outsider_client.register(&db.state, "outsider_user").await;
    let outsider_user = outsider_client.get_user(&db.state).await;

    let err = marketplace::execute(
        &db.state,
        &outsider_user,
        None,
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);

    // 2. Pending approval standing rejects publication (INV-018)
    sqlx::query("UPDATE suasobra.suppliers SET standing='pending_approval' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("standing"));

    // 3. Suspended standing rejects publication
    sqlx::query("UPDATE suasobra.suppliers SET standing='suspended' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // 4. Closed standing rejects publication
    sqlx::query("UPDATE suasobra.suppliers SET standing='closed' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // 5. Warned standing is eligible per UC-003
    sqlx::query("UPDATE suasobra.suppliers SET standing='warned' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let (status, _) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);

    db.finish().await;
}

#[tokio::test]
async fn displayed_price_alone_grants_no_price_lock_or_stock_priority() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "inv021_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Packaged Surplus Cartons".into(),
        description: "Misprinted cardboard cartons".into(),
        category: "packaging".into(),
        subcategory: None,
        unit: "bundle".into(),
        admitted_quantity: 15,
        surplus_reason: "Overrun packaging boxes from completed contract".into(),
        condition: "fresh".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Clean dry corrugated cardboard".into()),
            ..Default::default()
        },
        price_cents: Some(1500),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(6),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(8),
        price_cents: Some(1500),
    };

    let (status, _) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(pub_input),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);

    // Verify INV-021: Viewing / querying the published offer and its price
    // grants no lock, priority, or reserved stock allocation.
    let mut consumer_a = Client::new();
    consumer_a.register(&db.state, "consumer_a").await;
    let (code_a, val_a) = consumer_a
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(code_a, 200);
    assert_eq!(val_a["price_cents"], 1500);
    assert_eq!(val_a["available_quantity"], 15);
    assert_eq!(val_a["committed_quantity"], 0);

    let mut consumer_b = Client::new();
    consumer_b.register(&db.state, "consumer_b").await;
    let (code_b, val_b) = consumer_b
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(code_b, 200);
    assert_eq!(val_b["price_cents"], 1500);
    assert_eq!(val_b["available_quantity"], 15);
    assert_eq!(val_b["committed_quantity"], 0);

    // Verify buckets remained completely conserved and unallocated in database
    let offer_db = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(offer_db.available_quantity, 15);
    assert_eq!(offer_db.committed_quantity, 0);
    assert_eq!(offer_db.fulfilled_quantity, 0);
    assert_eq!(offer_db.disposed_quantity, 0);
    assert_eq!(offer_db.admitted_quantity, 15);

    db.finish().await;
}

#[tokio::test]
async fn rejects_unsafe_or_expired_food_and_exceeding_hard_safety_limit() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "safety_pub_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();
    let short_expiry = now + Duration::hours(5);

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Perishable Dairy Surplus".into(),
        description: "Fresh milk surplus".into(),
        category: "food".into(),
        subcategory: Some("dairy".into()),
        unit: "bottle".into(),
        admitted_quantity: 8,
        surplus_reason: "Over-order of fresh milk for hotel breakfast".into(),
        condition: "near_expiry".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Keep refrigerated below 4C".into()),
            age_or_preparation: Some("Pasteurized 3 days ago".into()),
            expiry_or_best_before: Some(short_expiry),
            allergens_or_dietary: Some("Contains milk/lactose".into()),
            safe_handling: None,
            condition_notes: Some("Sealed bottles".into()),
            packaging_integrity: Some("Intact cap".into()),
        },
        price_cents: Some(300),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    // 1. Pickup window exceeding hard safety limit is rejected (INV-028)
    let bad_pickup_pub = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(4),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: short_expiry + Duration::hours(2), // Exceeds expiry!
        price_cents: Some(300),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_pickup_pub),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("expiry"));

    // 2. Booking deadline exceeding hard safety limit is rejected
    let bad_booking_pub = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: short_expiry + Duration::hours(1), // Exceeds expiry!
        pickup_starts_at: short_expiry,
        pickup_ends_at: short_expiry + Duration::hours(2),
        price_cents: Some(300),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_booking_pub),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("expiry"));

    // 3. Expired food offer is rejected
    sqlx::query(
        "UPDATE suasobra.offers SET safety_disclosures = jsonb_set(safety_disclosures, '{expiry_or_best_before}', to_jsonb(now() - interval '1 hour')) WHERE id=$1",
    )
    .bind(offer_id)
    .execute(&db.state.pool)
    .await
    .unwrap();

    let expired_pub = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(2),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(3),
        price_cents: Some(300),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(expired_pub),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("Expired food"));

    db.finish().await;
}

#[tokio::test]
async fn rejects_invalid_intervals() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "interval_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Textile Cotton Offcuts".into(),
        description: "Fabric scrap rolls".into(),
        category: "textiles_offcuts".into(),
        subcategory: None,
        unit: "roll".into(),
        admitted_quantity: 3,
        surplus_reason: "End-of-bolt cotton fabric offcuts from tailor".into(),
        condition: "clean_offcuts".into(),
        safety_disclosures: SafetyDisclosures::default(),
        price_cents: Some(1500),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    // 1. booking_ends_at <= booking_starts_at
    let bad_intervals_1 = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now + Duration::hours(5)),
        booking_ends_at: now + Duration::hours(3), // Closing before opening!
        pickup_starts_at: now + Duration::hours(6),
        pickup_ends_at: now + Duration::hours(8),
        price_cents: Some(1500),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_intervals_1),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // 2. pickup_ends_at <= pickup_starts_at
    let bad_intervals_2 = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(4),
        pickup_starts_at: now + Duration::hours(6),
        pickup_ends_at: now + Duration::hours(5), // Ends before starts!
        price_cents: Some(1500),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_intervals_2),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // 3. booking_ends_at > pickup_ends_at (Booking closing cannot exceed pickup end)
    let bad_intervals_3 = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(8), // Closes after pickup ends!
        pickup_starts_at: now + Duration::hours(2),
        pickup_ends_at: now + Duration::hours(6),
        price_cents: Some(1500),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_intervals_3),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    // 4. pickup_starts_at < booking_starts_at (Pickup opening cannot precede booking opening)
    let bad_intervals_4 = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now + Duration::hours(3)),
        booking_ends_at: now + Duration::hours(6),
        pickup_starts_at: now + Duration::hours(1), // Pickup opening before booking opening!
        pickup_ends_at: now + Duration::hours(7),
        price_cents: Some(1500),
    };
    let err = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::PublishOffer(bad_intervals_4),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);

    db.finish().await;
}

#[tokio::test]
async fn http_routes_for_draft_publication_and_retrieval() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "http_pub_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;

    let now = Utc::now();
    let expiry = now + Duration::hours(24);

    // 1. Create draft via HTTP POST /api/v1/offers/
    let draft_payload = json!({
        "supplier_id": supplier_id,
        "establishment_id": establishment_id,
        "pickup_point_id": pickup_point_id,
        "title": "Bakery Morning Brioche",
        "description": "Extra brioche buns",
        "category": "food",
        "unit": "pack",
        "admitted_quantity": 12,
        "surplus_reason": "Morning forecast overage in bakery display",
        "condition": "prepared_today",
        "safety_disclosures": {
            "storage_instructions": "Store dry",
            "age_or_preparation": "Baked at 06:00",
            "expiry_or_best_before": expiry,
            "allergens_or_dietary": "Eggs, wheat, milk"
        },
        "price_cents": 600
    });

    let (draft_code, draft_res) = owner_client
        .call(&db.state, "POST", "/api/v1/offers/", draft_payload)
        .await;
    assert_eq!(draft_code, 201, "{draft_res}");
    let offer_id = draft_res["id"].as_str().unwrap();

    // 2. Publish offer via HTTP POST /api/v1/offers/{id}/publish/
    let pub_payload = json!({
        "booking_ends_at": (now + Duration::hours(6)),
        "pickup_starts_at": (now + Duration::hours(1)),
        "pickup_ends_at": (now + Duration::hours(8)),
        "price_cents": 500
    });

    let (pub_code, pub_res) = owner_client
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/offers/{offer_id}/publish/"),
            pub_payload,
        )
        .await;
    assert_eq!(pub_code, 200, "{pub_res}");
    assert_eq!(pub_res["status"], "published");
    assert_eq!(pub_res["price_cents"], 500);

    // 3. Retrieve offer via HTTP GET /api/v1/offers/{id}/
    let (get_code, get_res) = owner_client
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(get_code, 200, "{get_res}");
    assert_eq!(get_res["id"], offer_id);
    assert_eq!(get_res["status"], "published");

    // 4. Retrieve supplier offers list via HTTP GET /api/v1/suppliers/{id}/offers/
    let (list_code, list_res) = owner_client
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/suppliers/{supplier_id}/offers/"),
            json!({}),
        )
        .await;
    assert_eq!(list_code, 200, "{list_res}");
    assert!(!list_res.as_array().unwrap().is_empty());

    db.finish().await;
}

#[tokio::test]
async fn command_replay_and_rollback_for_publish() {
    let db = Database::new().await;
    let mut owner_client = Client::new();
    owner_client.register(&db.state, "replay_pub_owner").await;
    let supplier_id = owner_client.supplier(&db.state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    let establishment_id = owner_client.establishment(&db.state, supplier_id).await;
    let pickup_point_id = owner_client.point(&db.state, establishment_id).await;
    let owner_user = owner_client.get_user(&db.state).await;

    let now = Utc::now();

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: "Clean Workshop Scrap Metal".into(),
        description: "Aluminium scrap bars".into(),
        category: "workshop_parts".into(),
        subcategory: None,
        unit: "piece".into(),
        admitted_quantity: 7,
        surplus_reason: "Clean leftover aluminium extrusions from fabrication".into(),
        condition: "fresh".into(),
        safety_disclosures: SafetyDisclosures {
            safe_handling: Some("Smooth edges, non-hazardous metal".into()),
            ..Default::default()
        },
        price_cents: Some(1800),
    };

    let (_, draft_val) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now),
        booking_ends_at: now + Duration::hours(5),
        pickup_starts_at: now + Duration::hours(1),
        pickup_ends_at: now + Duration::hours(6),
        price_cents: Some(1800),
    };

    // First call with idempotency key
    let (code_1, val_1) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("replay-pub-key-1"),
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(code_1, StatusCode::OK);

    // Exact replay with same key returns cached 200 OK
    let (code_2, val_2) = marketplace::execute(
        &db.state,
        &owner_user,
        Some("replay-pub-key-1"),
        Command::PublishOffer(pub_input.clone()),
    )
    .await
    .unwrap();
    assert_eq!(code_2, StatusCode::OK);
    assert_eq!(val_1, val_2);

    // Conflict on key reuse with different payload
    let mut modified_pub = pub_input.clone();
    modified_pub.price_cents = Some(2200);

    let err = marketplace::execute(
        &db.state,
        &owner_user,
        Some("replay-pub-key-1"),
        Command::PublishOffer(modified_pub),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);

    db.finish().await;
}

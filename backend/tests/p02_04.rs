mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use suasobra_backend::marketplace::{
    self, Command, CreateDraftInput, PublishOfferInput, SafetyDisclosures,
};
use support::*;
use uuid::Uuid;

async fn setup_supplier_and_site(
    state: &suasobra_backend::AppState,
    owner_client: &mut Client,
    name: &str,
) -> (Uuid, Uuid) {
    owner_client.register(state, name).await;
    let supplier_id = owner_client.supplier(state).await;
    sqlx::query("UPDATE suasobra.suppliers SET standing='active' WHERE id=$1")
        .bind(supplier_id)
        .execute(&state.pool)
        .await
        .unwrap();
    let establishment_id = owner_client.establishment(state, supplier_id).await;
    (supplier_id, establishment_id)
}

async fn create_custom_pickup_point(
    state: &suasobra_backend::AppState,
    owner_client: &mut Client,
    establishment_id: Uuid,
    name: &str,
    lat: &str,
    lon: &str,
    is_private: bool,
) -> Uuid {
    let payload = json!({
        "name": name,
        "address_street": "Rua das Flores",
        "address_number": "123",
        "address_neighborhood": "Bairro Central",
        "address_city": "São Paulo",
        "address_state": "SP",
        "address_postal_code": "01000-000",
        "latitude": lat,
        "longitude": lon,
        "time_zone": "America/Sao_Paulo",
        "service_intervals": [
            {"day_of_week": 0, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 1, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 2, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 3, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 4, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 5, "start_time": "08:00", "end_time": "18:00"},
            {"day_of_week": 6, "start_time": "08:00", "end_time": "18:00"}
        ],
        "is_private_address": is_private,
        "access_instructions": "Enter through rear gate with code 1234",
        "address_complement": "Apt 4B"
    });

    let (status, value) = owner_client
        .call(
            state,
            "POST",
            &format!("/api/v1/establishments/{establishment_id}/pickup-points/"),
            payload,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["id"].as_str().unwrap().parse().unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn create_and_publish_offer(
    state: &suasobra_backend::AppState,
    owner_client: &mut Client,
    supplier_id: Uuid,
    establishment_id: Uuid,
    pickup_point_id: Uuid,
    title: &str,
    category: &str,
    condition: &str,
    admitted_qty: u64,
    price_cents: u64,
    booking_start_offset: Duration,
    booking_end_offset: Duration,
    pickup_start_offset: Duration,
    pickup_end_offset: Duration,
    expiry_offset: Duration,
) -> Uuid {
    let now = Utc::now();
    let owner_user = owner_client.get_user(state).await;

    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id,
        title: title.into(),
        description: format!("Surplus item: {title}"),
        category: category.into(),
        subcategory: None,
        unit: "unit".into(),
        admitted_quantity: admitted_qty,
        surplus_reason: "Daily surplus from production overage".into(),
        condition: condition.into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: if category == "food" {
                Some("Keep refrigerated".into())
            } else {
                None
            },
            age_or_preparation: if category == "food" {
                Some("Prepared today".into())
            } else {
                None
            },
            expiry_or_best_before: if category == "food" {
                Some(now + expiry_offset)
            } else {
                None
            },
            allergens_or_dietary: if category == "food" {
                Some("None".into())
            } else {
                None
            },
            safe_handling: None,
            condition_notes: Some("Inspected and intact".into()),
            packaging_integrity: Some("Original sealed".into()),
        },
        price_cents: Some(price_cents),
    };

    let (draft_status, draft_val) =
        marketplace::execute(state, &owner_user, None, Command::CreateDraft(draft_input))
            .await
            .unwrap();
    assert_eq!(draft_status, StatusCode::CREATED);
    let offer_id: Uuid = draft_val["id"].as_str().unwrap().parse().unwrap();

    let pub_input = PublishOfferInput {
        offer_id: Some(offer_id),
        booking_starts_at: Some(now + booking_start_offset),
        booking_ends_at: now + booking_end_offset,
        pickup_starts_at: now + pickup_start_offset,
        pickup_ends_at: now + pickup_end_offset,
        price_cents: Some(price_cents),
    };

    let (pub_status, _) =
        marketplace::execute(state, &owner_user, None, Command::PublishOffer(pub_input))
            .await
            .unwrap();
    assert_eq!(pub_status, StatusCode::OK);

    offer_id
}

#[tokio::test]
async fn discovery_matches_active_published_offers_within_radius_and_sorts_deterministically() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_1").await;

    // Point 1: Paulista (~2.7 km from Praça da Sé: -23.5505, -46.6333)
    let p1 = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Paulista Site",
        "-23.561414",
        "-46.655881",
        false,
    )
    .await;

    // Point 2: Mooca (~3.6 km from Praça da Sé)
    let p2 = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Mooca Site",
        "-23.556200",
        "-46.598300",
        false,
    )
    .await;

    let offer_paulista = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        p1,
        "Paulista Bread",
        "food",
        "prepared_today",
        10,
        1200,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(30),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    let offer_mooca = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        p2,
        "Mooca Croissants",
        "food",
        "prepared_today",
        5,
        1500,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(30),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // Anonymous visitor discovers around Praça da Sé with 5 km radius
    let mut visitor = Client::new();
    let (status, response) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=5.0",
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["total"], 2);
    let items = response["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // Sorted by distance ascending: Paulista (~2.7km) first, then Mooca (~3.6km)
    assert_eq!(items[0]["id"], offer_paulista.to_string());
    assert_eq!(items[1]["id"], offer_mooca.to_string());
    assert!(
        items[0]["approximate_distance_km"].as_f64().unwrap()
            <= items[1]["approximate_distance_km"].as_f64().unwrap()
    );
}

#[tokio::test]
async fn discovery_excludes_offers_outside_radius() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_2").await;

    // Point Near: Sé (~0.2 km from query)
    let p_near = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Near Site",
        "-23.551000",
        "-46.634000",
        false,
    )
    .await;

    // Point Far: Campinas (~85 km from Sé)
    let p_far = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Campinas Site",
        "-22.905600",
        "-47.060800",
        false,
    )
    .await;

    let offer_near = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        p_near,
        "Near Surplus",
        "food",
        "prepared_today",
        8,
        900,
        Duration::minutes(-5),
        Duration::hours(5),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    let _offer_far = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        p_far,
        "Far Surplus",
        "food",
        "prepared_today",
        8,
        900,
        Duration::minutes(-5),
        Duration::hours(5),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    let mut visitor = Client::new();
    let (status, response) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=10.0",
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["total"], 1);
    let items = response["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], offer_near.to_string());
}

#[tokio::test]
async fn discovery_excludes_unpublished_scheduled_and_ended_time_offers() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_3").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Time Check Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    // 1. Valid published offer
    let live_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Live Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-10),
        Duration::hours(4),
        Duration::minutes(30),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 2. Draft offer (never published)
    let owner_user = owner.get_user(&db.state).await;
    let draft_input = CreateDraftInput {
        supplier_id,
        establishment_id,
        pickup_point_id: point_id,
        title: "Draft Only Offer".into(),
        description: "Should not appear".into(),
        category: "food".into(),
        subcategory: None,
        unit: "unit".into(),
        admitted_quantity: 5,
        surplus_reason: "Surplus production test".into(),
        condition: "prepared_today".into(),
        safety_disclosures: SafetyDisclosures {
            storage_instructions: Some("Ambient".into()),
            age_or_preparation: Some("Today".into()),
            expiry_or_best_before: Some(Utc::now() + Duration::hours(24)),
            allergens_or_dietary: Some("None".into()),
            safe_handling: None,
            condition_notes: None,
            packaging_integrity: None,
        },
        price_cents: Some(1000),
    };
    let (_, draft_res) = marketplace::execute(
        &db.state,
        &owner_user,
        None,
        Command::CreateDraft(draft_input),
    )
    .await
    .unwrap();
    let draft_id: Uuid = draft_res["id"].as_str().unwrap().parse().unwrap();

    // 3. Scheduled future offer (starts in 2 hours)
    let _scheduled_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Scheduled Future Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::hours(2),
        Duration::hours(6),
        Duration::hours(3),
        Duration::hours(8),
        Duration::hours(24),
    )
    .await;

    // 4. Offer with expired booking window (booking_ends_at in past)
    let expired_booking_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Expired Booking Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::hours(-4),
        Duration::hours(2),
        Duration::hours(-2),
        Duration::hours(4),
        Duration::hours(24),
    )
    .await;
    sqlx::query(
        "UPDATE suasobra.offers SET booking_ends_at = now() - interval '10 minutes' WHERE id = $1",
    )
    .bind(expired_booking_offer)
    .execute(&db.state.pool)
    .await
    .unwrap();

    // 5. Offer with expired pickup window (pickup_ends_at in past)
    let expired_pickup_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Expired Pickup Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::hours(-4),
        Duration::hours(2),
        Duration::hours(-2),
        Duration::hours(4),
        Duration::hours(24),
    )
    .await;
    sqlx::query(
        "UPDATE suasobra.offers SET pickup_ends_at = now() - interval '10 minutes' WHERE id = $1",
    )
    .bind(expired_pickup_offer)
    .execute(&db.state.pool)
    .await
    .unwrap();

    // Discovery query
    let mut visitor = Client::new();
    let (status, response) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=5.0",
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let items = response["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "Only live_offer should be discovered");
    assert_eq!(items[0]["id"], live_offer.to_string());

    // Verify draft offer returns 404 to outsider on direct retrieval
    let (draft_get_status, _) = visitor
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{draft_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(draft_get_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn discovery_excludes_sold_out_offers_and_inactive_sites() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_4").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Stock Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    // 1. Live available offer
    let live_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "In Stock Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 2. Sold out offer (available_quantity = 0)
    let sold_out_offer = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Sold Out Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity = 0, committed_quantity = 10 WHERE id = $1",
    )
    .bind(sold_out_offer)
    .execute(&db.state.pool)
    .await
    .unwrap();

    // 3. Offer on inactive establishment
    let est_inactive = owner.establishment(&db.state, supplier_id).await;
    let p_inactive_est = create_custom_pickup_point(
        &db.state,
        &mut owner,
        est_inactive,
        "Inactive Est Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;
    let offer_inactive_est = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        est_inactive,
        p_inactive_est,
        "Inactive Est Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;
    sqlx::query("UPDATE suasobra.establishments SET is_active = false WHERE id = $1")
        .bind(est_inactive)
        .execute(&db.state.pool)
        .await
        .unwrap();

    // 4. Offer on inactive pickup point
    let p_inactive = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Inactive Point Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;
    let offer_inactive_point = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        p_inactive,
        "Inactive Point Offer",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;
    sqlx::query("UPDATE suasobra.pickup_points SET is_active = false WHERE id = $1")
        .bind(p_inactive)
        .execute(&db.state.pool)
        .await
        .unwrap();

    // Discovery query
    let mut visitor = Client::new();
    let (status, response) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=5.0",
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let items = response["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "Only live available offer should appear");
    assert_eq!(items[0]["id"], live_offer.to_string());

    // Inactive sites return 404 to outsiders on direct detail lookup
    let (st_est, _) = visitor
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_inactive_est}/"),
            json!({}),
        )
        .await;
    assert_eq!(st_est, StatusCode::NOT_FOUND);

    let (st_pt, _) = visitor
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_inactive_point}/"),
            json!({}),
        )
        .await;
    assert_eq!(st_pt, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn discovery_excludes_expired_food_and_untruthful_conditions() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_5").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Food Safety Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    // 1. Safe food with future expiry
    let safe_food = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Fresh Bread",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 2. Food offer with past expiry
    let expired_food = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Expired Food",
        "food",
        "prepared_today",
        10,
        1000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;
    sqlx::query(
        "UPDATE suasobra.offers SET safety_disclosures = jsonb_set(safety_disclosures, '{expiry_or_best_before}', to_jsonb((now() - interval '1 hour')::text)) WHERE id = $1"
    )
    .bind(expired_food)
    .execute(&db.state.pool)
    .await
    .unwrap();

    // 3. Non-food offer (construction wood)
    let non_food = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Wood Offcuts",
        "wood_carpentry",
        "clean_offcuts",
        20,
        3000,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    let mut visitor = Client::new();
    let (status, response) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=5.0",
            json!({}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let items = response["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        2,
        "Expired food excluded, safe food and non-food present"
    );

    let ids: Vec<String> = items
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&safe_food.to_string()));
    assert!(ids.contains(&non_food.to_string()));
    assert!(!ids.contains(&expired_food.to_string()));
}

#[tokio::test]
async fn discovery_category_and_price_filters_work() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_6").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Filter Test Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    // 1. Food offer at 1500 cents
    let food_paid = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Artisanal Pastry",
        "food",
        "prepared_today",
        10,
        1500,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 2. Food offer at 0 cents (free)
    let food_free = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Surplus Baguettes",
        "food",
        "prepared_today",
        8,
        0,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 3. Wood carpentry offer at 2500 cents
    let wood_paid = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Plywood Scraps",
        "wood_carpentry",
        "clean_offcuts",
        5,
        2500,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    let mut visitor = Client::new();

    // Filter by category=food
    let (_, res_food) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&category=food",
            json!({}),
        )
        .await;
    assert_eq!(res_food["total"], 2);
    let ids: Vec<String> = res_food["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&food_paid.to_string()));
    assert!(ids.contains(&food_free.to_string()));

    // Filter by category=wood_carpentry
    let (_, res_wood) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&category=wood_carpentry",
            json!({}),
        )
        .await;
    assert_eq!(res_wood["total"], 1);
    assert_eq!(res_wood["items"][0]["id"], wood_paid.to_string());

    // Filter by free_only=true
    let (_, res_free) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&free_only=true",
            json!({}),
        )
        .await;
    assert_eq!(res_free["total"], 1);
    assert_eq!(res_free["items"][0]["id"], food_free.to_string());

    // Filter by max_price_cents=1500 (should include 0 and 1500, exclude 2500)
    let (_, res_price) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&max_price_cents=1500",
            json!({}),
        )
        .await;
    assert_eq!(res_price["total"], 2);
    let price_ids: Vec<String> = res_price["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert!(price_ids.contains(&food_paid.to_string()));
    assert!(price_ids.contains(&food_free.to_string()));
    assert!(!price_ids.contains(&wood_paid.to_string()));
}

#[tokio::test]
async fn discovery_bounded_pagination_and_query_validation() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_7").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Pagination Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    // Create 5 offers
    let mut offer_ids = Vec::new();
    for i in 1..=5 {
        let id = create_and_publish_offer(
            &db.state,
            &mut owner,
            supplier_id,
            establishment_id,
            point_id,
            &format!("Offer {i}"),
            "food",
            "prepared_today",
            10,
            1000 + i * 100,
            Duration::minutes(-5),
            Duration::hours(4),
            Duration::minutes(15),
            Duration::hours(6),
            Duration::hours(24),
        )
        .await;
        offer_ids.push(id);
    }

    let mut visitor = Client::new();

    // Page 1: limit 2, offset 0
    let (st1, res1) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&limit=2&offset=0",
            json!({}),
        )
        .await;
    assert_eq!(st1, StatusCode::OK);
    assert_eq!(res1["total"], 5);
    assert_eq!(res1["items"].as_array().unwrap().len(), 2);

    // Page 2: limit 2, offset 2
    let (st2, res2) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&limit=2&offset=2",
            json!({}),
        )
        .await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(res2["total"], 5);
    assert_eq!(res2["items"].as_array().unwrap().len(), 2);

    // Items on page 1 and page 2 must be disjoint
    let p1_ids: Vec<String> = res1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    let p2_ids: Vec<String> = res2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    for id in &p1_ids {
        assert!(!p2_ids.contains(id), "Disjoint pages required");
    }

    // Page 3: limit 2, offset 4
    let (st3, res3) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&limit=2&offset=4",
            json!({}),
        )
        .await;
    assert_eq!(st3, StatusCode::OK);
    assert_eq!(res3["total"], 5);
    assert_eq!(res3["items"].as_array().unwrap().len(), 1);

    // Page 4: beyond results
    let (st4, res4) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&limit=2&offset=6",
            json!({}),
        )
        .await;
    assert_eq!(st4, StatusCode::OK);
    assert_eq!(res4["total"], 5);
    assert_eq!(res4["items"].as_array().unwrap().len(), 0);

    // Boundary validations
    let invalid_queries = [
        ("latitude=95.0&longitude=-46.6333", "Latitude out of bounds"),
        (
            "latitude=-23.5505&longitude=-200.0",
            "Longitude out of bounds",
        ),
        (
            "latitude=-23.5505&longitude=-46.6333&radius_km=0.05",
            "Radius < 0.1",
        ),
        (
            "latitude=-23.5505&longitude=-46.6333&radius_km=60.0",
            "Radius > 50.0",
        ),
        ("latitude=-23.5505&longitude=-46.6333&limit=0", "Limit = 0"),
        (
            "latitude=-23.5505&longitude=-46.6333&limit=101",
            "Limit > 100",
        ),
        (
            "latitude=-23.5505&longitude=-46.6333&category=",
            "Category empty",
        ),
    ];

    for (q, label) in invalid_queries {
        let (err_st, err_res) = visitor
            .call(
                &db.state,
                "GET",
                &format!("/api/v1/offers/discover/?{q}"),
                json!({}),
            )
            .await;
        assert_eq!(
            err_st,
            StatusCode::BAD_REQUEST,
            "Failed check: {label} -> {err_res}"
        );
    }
}

#[tokio::test]
async fn discovery_and_detail_enforce_location_privacy_and_mask_private_addresses() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_8").await;

    // Create private pickup point with sensitive coordinates and instructions
    let private_point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Private Residential Point",
        "-23.550520",
        "-46.633308",
        true, // is_private_address
    )
    .await;

    let offer_id = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        private_point_id,
        "Private Point Surplus",
        "food",
        "prepared_today",
        10,
        1500,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // 1. Anonymous visitor discovers offer
    let mut visitor = Client::new();
    let (st_disc, res_disc) = visitor
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.550520&longitude=-46.633308&radius_km=5.0",
            json!({}),
        )
        .await;

    assert_eq!(st_disc, StatusCode::OK);
    let item = &res_disc["items"][0];
    let pt = &item["pickup_point"];

    // INV-057 assertions on discovery:
    assert!(
        pt.get("latitude").is_none(),
        "Exact latitude must not leak in discovery"
    );
    assert!(
        pt.get("longitude").is_none(),
        "Exact longitude must not leak in discovery"
    );
    assert!(
        pt.get("access_instructions").is_none(),
        "Private instructions must not leak in discovery"
    );
    assert!(
        pt.get("address_complement").is_none(),
        "Complement must not leak in discovery"
    );
    assert_eq!(
        pt["address_street"], "[Private location revealed after reservation]",
        "Private address must be masked in discovery"
    );
    assert_eq!(
        pt["address_number"], "",
        "Address number must be stripped for private address"
    );
    assert!(item.get("approximate_distance_km").is_some());

    // 2. Anonymous visitor / consumer views offer detail via GET /api/v1/offers/{id}/
    let (st_detail_pub, res_detail_pub) = visitor
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;

    assert_eq!(st_detail_pub, StatusCode::OK);
    let pub_pt = &res_detail_pub["pickup_point"];
    assert!(pub_pt.get("latitude").is_none());
    assert!(pub_pt.get("longitude").is_none());
    assert!(pub_pt.get("access_instructions").is_none());
    assert_eq!(
        pub_pt["address_street"],
        "[Private location revealed after reservation]"
    );
    assert_eq!(pub_pt["address_number"], "");

    // 3. Supplier owner views offer detail -> gets full unmasked data
    let (st_owner_detail, res_owner_detail) = owner
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;

    assert_eq!(st_owner_detail, StatusCode::OK);
    let owner_pt = &res_owner_detail["pickup_point"];
    assert_eq!(owner_pt["latitude"], "-23.550520");
    assert_eq!(owner_pt["longitude"], "-46.633308");
    assert_eq!(
        owner_pt["access_instructions"],
        "Enter through rear gate with code 1234"
    );
    assert_eq!(owner_pt["address_street"], "Rua das Flores");
    assert_eq!(owner_pt["address_number"], "123");
}

#[tokio::test]
async fn discovery_and_viewing_does_not_mutate_inventory_or_grant_lock() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, establishment_id) =
        setup_supplier_and_site(&db.state, &mut owner, "disc_owner_9").await;

    let point_id = create_custom_pickup_point(
        &db.state,
        &mut owner,
        establishment_id,
        "Conservation Site",
        "-23.550500",
        "-46.633300",
        false,
    )
    .await;

    let offer_id = create_and_publish_offer(
        &db.state,
        &mut owner,
        supplier_id,
        establishment_id,
        point_id,
        "Conserved Bread",
        "food",
        "prepared_today",
        15,
        1800,
        Duration::minutes(-5),
        Duration::hours(4),
        Duration::minutes(15),
        Duration::hours(6),
        Duration::hours(24),
    )
    .await;

    // Check baseline inventory in database
    let initial_offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(initial_offer.admitted_quantity, 15);
    assert_eq!(initial_offer.available_quantity, 15);
    assert_eq!(initial_offer.committed_quantity, 0);
    assert_eq!(initial_offer.fulfilled_quantity, 0);
    assert_eq!(initial_offer.disposed_quantity, 0);
    assert_eq!(initial_offer.price_cents, 1800);

    // Consumer A discovers offer
    let mut consumer_a = Client::new();
    consumer_a.register(&db.state, "consumer_viewer_a").await;
    let (st_a, res_a) = consumer_a
        .call(
            &db.state,
            "GET",
            "/api/v1/offers/discover/?latitude=-23.5505&longitude=-46.6333&radius_km=5.0",
            json!({}),
        )
        .await;
    assert_eq!(st_a, StatusCode::OK);
    assert_eq!(res_a["items"][0]["id"], offer_id.to_string());

    // Consumer B views offer detail
    let mut consumer_b = Client::new();
    consumer_b.register(&db.state, "consumer_viewer_b").await;
    let (st_b, res_b) = consumer_b
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(st_b, StatusCode::OK);
    assert_eq!(res_b["id"], offer_id.to_string());
    assert_eq!(res_b["price_cents"], 1800);

    // Verify INV-021: Inventory completely unchanged in database
    let after_view_offer = marketplace::get_offer(&db.state.pool, offer_id)
        .await
        .unwrap();
    assert_eq!(after_view_offer.admitted_quantity, 15);
    assert_eq!(after_view_offer.available_quantity, 15);
    assert_eq!(after_view_offer.committed_quantity, 0);
    assert_eq!(after_view_offer.fulfilled_quantity, 0);
    assert_eq!(after_view_offer.disposed_quantity, 0);
    assert_eq!(after_view_offer.price_cents, 1800);

    // Verify no lock or replay was recorded
    let _replay_count: i64 = sqlx::query_scalar("SELECT count(*) FROM suasobra.command_replays")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    // Only register/setup replays may exist, no reservation replay exists
    let res_replay_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM suasobra.command_replays WHERE command_name LIKE '%reserve%'",
    )
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(
        res_replay_count, 0,
        "No reservation replays created by discovery/viewing"
    );

    // Supplier changes price to 1200
    sqlx::query("UPDATE suasobra.offers SET price_cents = 1200 WHERE id = $1")
        .bind(offer_id)
        .execute(&db.state.pool)
        .await
        .unwrap();

    // Consumer A views offer again -> sees new price 1200 (INV-021: no price lock was granted by previous view)
    let (_, res_a_new) = consumer_a
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/offers/{offer_id}/"),
            json!({}),
        )
        .await;
    assert_eq!(
        res_a_new["price_cents"], 1200,
        "Displayed price conferred no price lock"
    );
}

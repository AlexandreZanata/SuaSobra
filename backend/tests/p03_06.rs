mod support;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use suasobra_backend::{
    AppState,
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

/// A second application handle with an independent PostgreSQL connection pool
/// to the same test database, proving contention across real connections.
async fn independent_state(db: &Database) -> AppState {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db.state.config.database_url)
        .await
        .unwrap();
    AppState {
        pool,
        config: db.state.config.clone(),
        password_slots: Arc::new(tokio::sync::Semaphore::new(4)),
    }
}

async fn call(
    state: &suasobra_backend::AppState,
    actor: &Client,
    command: Command,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let user = actor.get_user(state).await;
    match reservations::execute(state, &user, key, command).await {
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

async fn action_count(state: &suasobra_backend::AppState, action: &str, entity: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.audit_facts WHERE action=$1 AND entity_type=$2",
    )
    .bind(action)
    .bind(entity)
    .fetch_one(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn independent_connections_race_for_last_units() {
    let db = Database::new().await;
    let peer = independent_state(&db).await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_pool").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Pool race lots",
            admitted: 2,
            price_cents: 500,
        },
    )
    .await;
    let mut first_buyer = Client::new();
    first_buyer.register(&db.state, "matrix_pool_a").await;
    let mut second_buyer = Client::new();
    second_buyer.register(&db.state, "matrix_pool_b").await;

    // INV-005 across independent connections: exactly one acceptance, and
    // units, state, snapshot and audit fully agree afterwards.
    let (a, b) = tokio::join!(
        call(
            &db.state,
            &first_buyer,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offer_id),
                quantity: 2,
            }),
            None,
        ),
        call(
            &peer,
            &second_buyer,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offer_id),
                quantity: 2,
            }),
            None,
        ),
    );
    let succeeded =
        usize::from(a.0 == StatusCode::CREATED) + usize::from(b.0 == StatusCode::CREATED);
    assert_eq!(succeeded, 1, "first={a:?} second={b:?}");
    assert_eq!(offer_buckets(&db.state, offer_id).await, (2, 0, 2, 0, 0));
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(
        action_count(&db.state, "reservation_confirmed", "reservation").await,
        1
    );
    db.finish().await;
}

#[tokio::test]
async fn reserve_cancel_race_keeps_everything_in_agreement() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_rc").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Race agreement lots",
            admitted: 5,
            price_cents: 500,
        },
    )
    .await;
    let mut holder = Client::new();
    holder.register(&db.state, "matrix_rc_holder").await;
    let mut challenger = Client::new();
    challenger.register(&db.state, "matrix_rc_challenger").await;
    let (_, val) = call(
        &db.state,
        &holder,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 3,
        }),
        None,
    )
    .await;
    let held_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    // Whoever wins, the persisted outcome must agree: buckets conserved, the
    // challenger holds a confirmed row exactly when 3 units are committed,
    // and the holder is always cancelled exactly once.
    let (cancel_out, reserve_out) = tokio::join!(
        call(
            &db.state,
            &holder,
            Command::CancelReservation(CancelReservationInput {
                reservation_id: Some(held_id),
                reason: "Racing cancellation with challenger".into(),
            }),
            None,
        ),
        call(
            &db.state,
            &challenger,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offer_id),
                quantity: 3,
            }),
            None,
        ),
    );
    assert_eq!(cancel_out.0, StatusCode::OK, "{cancel_out:?}");
    let (admitted, available, committed, fulfilled, disposed) =
        offer_buckets(&db.state, offer_id).await;
    assert_eq!(admitted, 5);
    assert_eq!(available + committed + fulfilled + disposed, admitted);
    let challenger_won = reserve_out.0 == StatusCode::CREATED;
    assert_eq!(committed, if challenger_won { 3 } else { 0 });
    assert_eq!(available, if challenger_won { 2 } else { 5 });
    let challenger_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.reservations WHERE consumer_id=$1 AND status='confirmed'",
    )
    .bind(challenger.user.unwrap())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(challenger_rows, i64::from(challenger_won));
    assert_eq!(
        action_count(&db.state, "reservation_cancelled", "reservation").await,
        1
    );
    db.finish().await;
}

#[tokio::test]
async fn cancel_expiry_race_disposes_or_releases_exactly_once() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_ce").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Cancel expiry lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut holder = Client::new();
    holder.register(&db.state, "matrix_ce_holder").await;
    let (_, val) = call(
        &db.state,
        &holder,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 2,
        }),
        None,
    )
    .await;
    let held_id: Uuid = val["id"].as_str().unwrap().parse().unwrap();

    // Race a holder cancellation against a far-future expiry sweep (which also
    // expires the offer itself): buckets are deterministic either way, and
    // exactly one terminal reservation audit exists.
    let (cancel_out, expire_out) = tokio::join!(
        call(
            &db.state,
            &holder,
            Command::CancelReservation(CancelReservationInput {
                reservation_id: Some(held_id),
                reason: "Racing cancellation against expiry".into(),
            }),
            None,
        ),
        reservations::expire_due(&db.state.pool, Utc::now() + Duration::hours(24)),
    );
    assert!(expire_out.is_ok());
    assert!(
        cancel_out.0 == StatusCode::OK || cancel_out.0 == StatusCode::CONFLICT,
        "{cancel_out:?}"
    );
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 0, 0, 0, 6));
    assert_eq!(action_count(&db.state, "offer_expired", "offer").await, 1);
    let status: String = sqlx::query_scalar("SELECT status FROM suasobra.reservations WHERE id=$1")
        .bind(held_id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    let cancelled = action_count(&db.state, "reservation_cancelled", "reservation").await;
    let expired = action_count(&db.state, "reservation_expired", "reservation").await;
    assert_eq!(cancelled + expired, 1);
    assert_eq!(
        status,
        if cancelled == 1 {
            "cancelled"
        } else {
            "expired"
        }
    );
    db.finish().await;
}

#[tokio::test]
async fn three_way_cap_race_admits_exactly_the_remaining_slots() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_cap").await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "matrix_cap_holder").await;

    // Launch policy allows 3 simultaneous commitments: pre-fill 1, then race
    // three parallel confirms for the 2 remaining slots.
    let filler = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Cap filler",
            admitted: 4,
            price_cents: 500,
        },
    )
    .await;
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(filler),
            quantity: 1,
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mut offers = vec![];
    for title in ["Cap race A", "Cap race B", "Cap race C"] {
        offers.push(
            create_published_offer(
                &db.state,
                &owner,
                OfferSetup {
                    supplier_id,
                    establishment_id: est_id,
                    point_id,
                    title,
                    admitted: 4,
                    price_cents: 500,
                },
            )
            .await,
        );
    }
    let (a, b, c) = tokio::join!(
        call(
            &db.state,
            &consumer,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offers[0]),
                quantity: 1,
            }),
            None,
        ),
        call(
            &db.state,
            &consumer,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offers[1]),
                quantity: 1,
            }),
            None,
        ),
        call(
            &db.state,
            &consumer,
            Command::ReserveNow(ReserveNowInput {
                offer_id: Some(offers[2]),
                quantity: 1,
            }),
            None,
        ),
    );
    let succeeded = [a, b, c]
        .iter()
        .filter(|(code, _)| *code == StatusCode::CREATED)
        .count();
    assert_eq!(succeeded, 2);

    // The consumer holds exactly 3 confirmed reservations; the losing offer
    // is untouched and every audit agrees.
    let held: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.reservations WHERE consumer_id=$1 AND status='confirmed'",
    )
    .bind(consumer.user.unwrap())
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(held, 3);
    assert_eq!(
        action_count(&db.state, "reservation_confirmed", "reservation").await,
        3
    );
    let mut untouched = 0;
    for offer_id in offers {
        if offer_buckets(&db.state, offer_id).await == (4, 4, 0, 0, 0) {
            untouched += 1;
        }
    }
    assert_eq!(untouched, 1);
    db.finish().await;
}

#[tokio::test]
async fn identical_key_race_persists_a_single_identity() {
    let db = Database::new().await;
    let peer = independent_state(&db).await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_key").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Key race lots",
            admitted: 6,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "matrix_key_holder").await;

    // INV-061 across independent connections: the identical concurrent retry
    // returns one identity with one stock move and one audit fact.
    let input = || {
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 2,
        })
    };
    let (a, b) = tokio::join!(
        call(&db.state, &consumer, input(), Some("matrix-key-1")),
        call(&peer, &consumer, input(), Some("matrix-key-1")),
    );
    assert_eq!(a.0, StatusCode::CREATED, "{a:?}");
    assert_eq!(b.0, StatusCode::CREATED, "{b:?}");
    assert_eq!(a.1["id"], b.1["id"]);
    assert_eq!(offer_buckets(&db.state, offer_id).await, (6, 4, 2, 0, 0));
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suasobra.reservations")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(
        action_count(&db.state, "reservation_confirmed", "reservation").await,
        1
    );
    db.finish().await;
}

#[tokio::test]
async fn mixed_lifecycle_matrix_reconciles_end_to_end() {
    let db = Database::new().await;
    let mut owner = Client::new();
    let (supplier_id, est_id, point_id) =
        setup_owner_with_site(&db.state, &mut owner, "matrix_mix").await;
    let offer_id = create_published_offer(
        &db.state,
        &owner,
        OfferSetup {
            supplier_id,
            establishment_id: est_id,
            point_id,
            title: "Matrix lots",
            admitted: 12,
            price_cents: 500,
        },
    )
    .await;
    let mut consumer = Client::new();
    consumer.register(&db.state, "matrix_mix_holder").await;

    // Confirm 2, confirm 1 (within the 3-unit per-offer cap), cancel the
    // first, then sweep far-future expiry: every unit, state, snapshot and
    // event must reconcile afterwards.
    let (status, first_val) = call(
        &db.state,
        &consumer,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 2,
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first_val}");
    let first_id: Uuid = first_val["id"].as_str().unwrap().parse().unwrap();
    let (status, second_val) = call(
        &db.state,
        &consumer,
        Command::ReserveNow(ReserveNowInput {
            offer_id: Some(offer_id),
            quantity: 1,
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second_val}");
    let second_id: Uuid = second_val["id"].as_str().unwrap().parse().unwrap();
    let (status, _) = call(
        &db.state,
        &consumer,
        Command::CancelReservation(CancelReservationInput {
            reservation_id: Some(first_id),
            reason: "Matrix cancellation with a clear reason".into(),
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let report = reservations::expire_due(&db.state.pool, Utc::now() + Duration::hours(48))
        .await
        .unwrap();
    assert_eq!(report.expired_reservations, 1);
    assert_eq!(report.expired_offers, 1);

    // Units: 12 admitted, all disposed (11 released-then-expired via the
    // offer sweep, 1 committed-then-expired via the reservation).
    assert_eq!(offer_buckets(&db.state, offer_id).await, (12, 0, 0, 0, 12));
    // States and snapshots: cancelled keeps its terms, expired keeps its own.
    let first: (String, i64, i64) = sqlx::query_as(
        "SELECT status, accepted_quantity, accepted_price_cents FROM suasobra.reservations WHERE id=$1",
    )
    .bind(first_id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!((first.0.as_str(), first.1, first.2), ("cancelled", 2, 500));
    let second: (String, i64, i64) = sqlx::query_as(
        "SELECT status, accepted_quantity, accepted_price_cents FROM suasobra.reservations WHERE id=$1",
    )
    .bind(second_id)
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!((second.0.as_str(), second.1, second.2), ("expired", 1, 500));
    // Events: the full chain agrees with the persisted states.
    assert_eq!(
        action_count(&db.state, "reservation_confirmed", "reservation").await,
        2
    );
    assert_eq!(
        action_count(&db.state, "reservation_cancelled", "reservation").await,
        1
    );
    assert_eq!(
        action_count(&db.state, "reservation_expired", "reservation").await,
        1
    );
    assert_eq!(action_count(&db.state, "offer_expired", "offer").await, 1);
    db.finish().await;
}

use crate::{
    AppState,
    audit::AuditFact,
    error::{AppError, Result},
    identity::{self, User},
    marketplace::Offer,
    primitives::{Authority, LockOrder, PolicyRegistry},
    replay::Replay,
};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

/// A confirmed launch reservation with its immutable accepted-term snapshot.
///
/// `quantity` always equals `accepted_quantity`: later offer or pickup-point
/// edits never rewrite the accepted columns (INV-020, INV-023). The database
/// trigger `reservations_snapshot_immutable` enforces this append-only rule.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Reservation {
    pub id: Uuid,
    pub offer_id: Uuid,
    pub supplier_id: Uuid,
    pub establishment_id: Uuid,
    pub pickup_point_id: Uuid,
    pub consumer_id: Uuid,
    pub quantity: i64,
    pub status: String,
    pub accepted_quantity: i64,
    pub accepted_price_cents: i64,
    pub accepted_currency: String,
    pub accepted_condition: String,
    pub accepted_pickup_point_version: i32,
    pub accepted_pickup_starts_at: DateTime<Utc>,
    pub accepted_pickup_ends_at: DateTime<Utc>,
    pub accepted_policy_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveNowInput {
    #[serde(default)]
    pub offer_id: Option<Uuid>,
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    ReserveNow(ReserveNowInput),
}

/// Reserve whole units on a published offer with immediate confirmation.
///
/// Launch scope (ADR-002, INV-030): free and at-pickup reservations confirm
/// immediately without any online payment state. The accepted price, quantity,
/// condition, pickup point version, pickup interval and policy version are
/// snapshotted into the reservation row and can never be rewritten (INV-020,
/// INV-023). The offer row lock serializes concurrent attempts so simultaneous
/// requests for the last unit yield at most one acceptance (INV-005).
pub async fn execute(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    command: Command,
) -> Result<(StatusCode, Value)> {
    let Command::ReserveNow(input) = command;
    let offer_id = input
        .offer_id
        .ok_or_else(|| AppError::validation("offer_id is required to reserve quantity"))?;
    if input.quantity == 0 {
        return Err(AppError::validation(
            "Reservation quantity must be a positive number of whole units",
        ));
    }

    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;

    let command_json = serde_json::to_value(&input).map_err(|_| AppError::internal())?;
    let (replay, cached) = Replay::begin(
        &mut tx,
        actor.id,
        "reserve_now",
        idempotency_key,
        &command_json,
    )
    .await?;
    if let Some((code, payload)) = cached {
        return Ok((
            StatusCode::from_u16(code).unwrap_or(StatusCode::OK),
            payload,
        ));
    }

    // Supplier-before-offer-before-reservation row locks: resolve the supplier
    // without locking, then lock supplier authority before the offer row.
    let supplier_id: Option<Uuid> =
        sqlx::query_scalar("SELECT supplier_id FROM suasobra.offers WHERE id=$1")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let supplier_id = supplier_id.ok_or_else(AppError::missing)?;

    let mut lock = LockOrder::default();
    lock.acquire(Authority::Supplier, &supplier_id.to_string())?;

    let supplier_row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT owner_id, standing FROM suasobra.suppliers WHERE id=$1 FOR UPDATE")
            .bind(supplier_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (owner_id, standing) = supplier_row.ok_or_else(AppError::missing)?;

    // Eligibility, self-dealing and suspension checks (INV-018, INV-019).
    if !actor.can_reserve(owner_id) {
        return Err(AppError::forbidden());
    }
    if standing != "active" && standing != "warned" {
        return Err(AppError::validation(format!(
            "Supplier standing '{standing}' is not eligible for reservations"
        )));
    }

    lock.acquire(Authority::Offer, &offer_id.to_string())?;
    let offer: Option<Offer> =
        sqlx::query_as("SELECT * FROM suasobra.offers WHERE id=$1 FOR UPDATE")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let offer = offer.ok_or_else(AppError::missing)?;

    // Only currently bookable published offers accept quantity (UC-005).
    if offer.status != "published" {
        return Err(AppError::conflict(format!(
            "Only published offers accept reservations; current status is '{}'",
            offer.status
        )));
    }

    let now = Utc::now();
    match (offer.booking_starts_at, offer.booking_ends_at) {
        (Some(starts), Some(ends)) if starts <= now && now < ends => {}
        _ => {
            return Err(AppError::conflict(
                "The booking interval is not open; stale availability grants no reservation right",
            ));
        }
    }
    match (offer.pickup_starts_at, offer.pickup_ends_at) {
        (Some(_), Some(ends)) if now < ends => {}
        _ => {
            return Err(AppError::conflict(
                "The pickup interval has ended; this offer can no longer be reserved",
            ));
        }
    }

    let est_active: Option<bool> = sqlx::query_scalar(
        "SELECT is_active FROM suasobra.establishments WHERE id=$1 AND supplier_id=$2 FOR UPDATE",
    )
    .bind(offer.establishment_id)
    .bind(offer.supplier_id)
    .fetch_optional(&mut *tx)
    .await?;
    if est_active != Some(true) {
        return Err(AppError::validation(
            "Establishment does not exist, does not belong to supplier, or is inactive",
        ));
    }

    let point_row: Option<(bool, i32)> = sqlx::query_as(
        "SELECT is_active, version FROM suasobra.pickup_points WHERE id=$1 AND establishment_id=$2 FOR UPDATE",
    )
    .bind(offer.pickup_point_id)
    .bind(offer.establishment_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (point_active, point_version) = point_row.ok_or_else(|| {
        AppError::validation("Pickup point does not exist or does not belong to establishment")
    })?;
    if !point_active {
        return Err(AppError::validation("Pickup point is inactive"));
    }

    // INV-004: a commitment never exceeds current available quantity.
    let requested = input.quantity as i64;
    if requested > offer.available_quantity {
        return Err(AppError::conflict(format!(
            "Only {} unit(s) remain available; rejected requests allocate no quantity",
            offer.available_quantity
        )));
    }

    // Account-wide and per-offer consumer caps from the active launch policy.
    let registry = PolicyRegistry::default();
    let policy = registry.active().ok_or_else(AppError::internal)?;
    let policy_version = policy.version.clone();
    let offer_consumed: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity),0)::bigint FROM suasobra.reservations WHERE consumer_id=$1 AND offer_id=$2 AND status='confirmed'",
    )
    .bind(actor.id)
    .bind(offer_id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if offer_consumed.unwrap_or(0) + requested
        > i64::from(policy.max_quantity_per_consumer_per_offer)
    {
        return Err(AppError::validation(format!(
            "Consumer offer cap exceeded: at most {} unit(s) per offer",
            policy.max_quantity_per_consumer_per_offer
        )));
    }
    let active_commitments: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM suasobra.reservations WHERE consumer_id=$1 AND status='confirmed'",
    )
    .bind(actor.id)
    .fetch_one(&mut *tx)
    .await?;
    if active_commitments >= i64::from(policy.max_total_simultaneous_commitments) {
        return Err(AppError::validation(format!(
            "Simultaneous commitment cap exceeded: at most {} active reservation(s)",
            policy.max_total_simultaneous_commitments
        )));
    }
    if offer.price_cents == 0 {
        let active_free: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM suasobra.reservations WHERE consumer_id=$1 AND status='confirmed' AND accepted_price_cents=0",
        )
        .bind(actor.id)
        .fetch_one(&mut *tx)
        .await?;
        if active_free >= i64::from(policy.max_simultaneous_free_reservations) {
            return Err(AppError::validation(format!(
                "Simultaneous free-reservation cap exceeded: at most {} active free reservation(s)",
                policy.max_simultaneous_free_reservations
            )));
        }
    }

    lock.acquire(Authority::Reservation, &offer_id.to_string())?;

    let new_available = offer.available_quantity - requested;
    let new_committed = offer
        .committed_quantity
        .checked_add(requested)
        .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?;
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity=$2, committed_quantity=$3, updated_at=now() WHERE id=$1",
    )
    .bind(offer_id)
    .bind(new_available)
    .bind(new_committed)
    .execute(&mut *tx)
    .await?;

    let pickup_starts_at = offer.pickup_starts_at.ok_or_else(AppError::internal)?;
    let pickup_ends_at = offer.pickup_ends_at.ok_or_else(AppError::internal)?;
    let reservation: Reservation = sqlx::query_as(
        r#"
        INSERT INTO suasobra.reservations (
            offer_id, supplier_id, establishment_id, pickup_point_id, consumer_id,
            quantity, status,
            accepted_quantity, accepted_price_cents, accepted_currency, accepted_condition,
            accepted_pickup_point_version, accepted_pickup_starts_at, accepted_pickup_ends_at,
            accepted_policy_version
        ) VALUES ($1,$2,$3,$4,$5,$6,'confirmed',$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING *
        "#,
    )
    .bind(offer_id)
    .bind(offer.supplier_id)
    .bind(offer.establishment_id)
    .bind(offer.pickup_point_id)
    .bind(actor.id)
    .bind(requested)
    .bind(offer.price_cents)
    .bind(offer.currency.clone())
    .bind(offer.condition.clone())
    .bind(point_version)
    .bind(pickup_starts_at)
    .bind(pickup_ends_at)
    .bind(policy_version.clone())
    .fetch_one(&mut *tx)
    .await?;

    let reservation_val: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM suasobra.reservations r WHERE id=$1")
            .bind(reservation.id)
            .fetch_one(&mut *tx)
            .await?;
    let prior_offer: Value = serde_json::to_value(&offer).unwrap_or(json!({}));

    let mut fact = AuditFact {
        entity_type: "reservation".into(),
        entity_id: reservation.id.to_string(),
        action: "reservation_confirmed".into(),
        actor_id: Some(actor.id),
        actor_scope: "consumer".into(),
        authority_or_policy: policy_version,
        evidence_kind: "participant_acknowledgment".into(),
        reason: format!(
            "Confirmed {} unit(s) of '{}' for direct {} at pickup",
            requested,
            offer.title,
            if offer.price_cents == 0 {
                "collection (free)"
            } else {
                "payment"
            }
        ),
        prior_state: prior_offer,
        resulting_state: reservation_val.clone(),
        prior_quantity: Some(offer.available_quantity),
        resulting_quantity: Some(new_available),
        quantity_delta: Some(-requested),
        money_delta_cents: Some(
            offer
                .price_cents
                .checked_mul(requested)
                .ok_or_else(|| AppError::validation("Goods value overflow"))?,
        ),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;

    replay.finish(&mut tx, 201, &reservation_val).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, reservation_val))
}

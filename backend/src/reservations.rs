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
use sqlx::PgPool;
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
#[serde(deny_unknown_fields)]
pub struct CancelReservationInput {
    #[serde(default)]
    pub reservation_id: Option<Uuid>,
    pub reason: String,
}

fn validate_cancel_reason(reason: &str) -> Result<String> {
    let trimmed = reason.trim();
    if trimmed.chars().count() < 5 {
        return Err(AppError::validation(
            "A cancellation reason of at least 5 characters is required (INV-012)",
        ));
    }
    if trimmed.chars().count() > 1000 {
        return Err(AppError::validation(
            "Cancellation reason must be at most 1000 characters",
        ));
    }
    Ok(trimmed.to_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    ReserveNow(ReserveNowInput),
    CancelReservation(CancelReservationInput),
}

/// Reserve whole units on a published offer with immediate confirmation.
///
/// Launch scope (ADR-002, INV-030): free and at-pickup reservations confirm
/// immediately without any online payment state. The accepted price, quantity,
/// condition, pickup point version, pickup interval and policy version are
/// snapshotted into the reservation row and can never be rewritten (INV-020,
/// INV-023). The consumer row lock is taken before any supplier or offer lock
/// so account-wide cap checks serialize per consumer, and the offer row lock
/// serializes concurrent attempts so simultaneous requests for the last unit
/// yield at most one acceptance (INV-005).
pub async fn execute(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    command: Command,
) -> Result<(StatusCode, Value)> {
    if let Command::CancelReservation(input) = command {
        return execute_cancel(state, actor, idempotency_key, input).await;
    }
    let Command::ReserveNow(input) = command else {
        return Err(AppError::internal());
    };
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

    // Consumer/cap authority first: the actor row lock above serializes this
    // consumer's cap checks before any supplier or offer lock is taken, so
    // parallel confirms on different offers cannot bypass account-wide caps
    // (INV-001, INV-005). Any future reordering that touches supplier
    // authority first fails loudly here instead of silently racing caps.
    let mut lock = LockOrder::default();
    lock.acquire(Authority::Consumer, &actor.id.to_string())?;

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

    // Consumer-before-supplier-before-offer-before-reservation row locks:
    // resolve the supplier without locking, then lock supplier authority
    // before the offer row.
    let supplier_id: Option<Uuid> =
        sqlx::query_scalar("SELECT supplier_id FROM suasobra.offers WHERE id=$1")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let supplier_id = supplier_id.ok_or_else(AppError::missing)?;

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

/// Participant-only reservation detail with staged private-point disclosure.
///
/// Only the consumer holder, the supplier owner or a moderator may read a
/// reservation: a bare resource UUID grants no access, and outsiders (or
/// anonymous callers) receive `404` so reservations are not an existence
/// oracle (INV-057). The accepted terms come straight from the immutable
/// snapshot columns, so later offer or point edits never move displayed
/// price, condition, times or policy (INV-020).
///
/// Exact pickup-point location is disclosed only at the permitted stage: the
/// consumer holder sees it while the reservation is `confirmed`; the
/// supplier owner (their own point) and moderators (case authority) see it
/// regardless of status. Any other stage returns the redacted public point.
pub async fn get_reservation_detail(
    pool: &PgPool,
    id: Uuid,
    viewer: Option<&User>,
) -> Result<Value> {
    #[derive(sqlx::FromRow)]
    struct DetailRow {
        reservation_val: Value,
        point_data: Value,
        supplier_owner_id: Uuid,
        consumer_id: Uuid,
        status: String,
        establishment_name: String,
    }

    let row: Option<DetailRow> = sqlx::query_as(
        r#"
        SELECT to_jsonb(r) AS reservation_val,
               p.data AS point_data,
               s.owner_id AS supplier_owner_id,
               r.consumer_id AS consumer_id,
               r.status AS status,
               e.name AS establishment_name
        FROM suasobra.reservations r
        JOIN suasobra.suppliers s ON r.supplier_id = s.id
        JOIN suasobra.pickup_points p ON r.pickup_point_id = p.id
        JOIN suasobra.establishments e ON r.establishment_id = e.id
        WHERE r.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(AppError::missing)?;
    let viewer = viewer.ok_or_else(AppError::missing)?;
    let is_holder = viewer.id == row.consumer_id;
    let is_owner = viewer.id == row.supplier_owner_id;
    if !is_holder && !is_owner && !viewer.moderator() {
        return Err(AppError::missing());
    }

    let mut detail = row.reservation_val;
    let disclose_exact = (row.status == "confirmed" && is_holder) || is_owner || viewer.moderator();
    let point = if disclose_exact {
        row.point_data
    } else {
        crate::suppliers::public_point(row.point_data)
    };
    if let Some(obj) = detail.as_object_mut() {
        obj.insert("pickup_point".into(), point);
        obj.insert(
            "establishment_name".into(),
            Value::String(row.establishment_name),
        );
    }
    Ok(detail)
}

/// Reservations held by one consumer: the consumer themselves or a moderator.
pub async fn list_consumer_reservations(
    pool: &PgPool,
    consumer_id: Uuid,
    viewer: &User,
) -> Result<Vec<Value>> {
    if viewer.id != consumer_id && !viewer.moderator() {
        return Err(AppError::missing());
    }
    sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM suasobra.reservations r WHERE r.consumer_id=$1 ORDER BY r.created_at DESC, r.id DESC",
    )
    .bind(consumer_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

/// Reservations on one supplier's offers: the supplier owner or a moderator.
pub async fn list_supplier_reservations(
    pool: &PgPool,
    supplier_id: Uuid,
    viewer: &User,
) -> Result<Vec<Value>> {
    let owner_id: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_id FROM suasobra.suppliers WHERE id=$1")
            .bind(supplier_id)
            .fetch_optional(pool)
            .await?;
    let owner_id = owner_id.ok_or_else(AppError::missing)?;
    if viewer.id != owner_id && !viewer.moderator() {
        return Err(AppError::missing());
    }
    sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM suasobra.reservations r WHERE r.supplier_id=$1 ORDER BY r.created_at DESC, r.id DESC",
    )
    .bind(supplier_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

/// Cancel a confirmed reservation with exact quantity effects.
///
/// The consumer holder, the supplier owner or a moderator may cancel with a
/// canonical reason. The reservation's exact committed units move back to
/// available exactly once (INV-007): only `confirmed` reservations can be
/// cancelled, so duplicate, late terminal or handover-after cancellations are
/// rejected without moving stock or auditing twice. Goods already handed over
/// (`picked_up`) never return automatically, and launch pays no refund
/// (ADR-002): cancellation only releases units, never money.
async fn execute_cancel(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    input: CancelReservationInput,
) -> Result<(StatusCode, Value)> {
    let reservation_id = input
        .reservation_id
        .ok_or_else(|| AppError::validation("reservation_id is required to cancel"))?;
    let reason = validate_cancel_reason(&input.reason)?;

    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;

    let mut lock = LockOrder::default();
    lock.acquire(Authority::Consumer, &actor.id.to_string())?;

    let command_json = serde_json::to_value(&input).map_err(|_| AppError::internal())?;
    let (replay, cached) = Replay::begin(
        &mut tx,
        actor.id,
        "cancel_reservation",
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

    // Consumer-before-supplier-before-offer-before-reservation row locks:
    // resolve linkage without locking, then lock in authority order.
    let linkage: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT offer_id, consumer_id FROM suasobra.reservations WHERE id=$1")
            .bind(reservation_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (offer_id, consumer_id) = linkage.ok_or_else(AppError::missing)?;

    let supplier_id: Option<Uuid> =
        sqlx::query_scalar("SELECT supplier_id FROM suasobra.offers WHERE id=$1")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let supplier_id = supplier_id.ok_or_else(AppError::missing)?;

    lock.acquire(Authority::Supplier, &supplier_id.to_string())?;
    let owner_id: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_id FROM suasobra.suppliers WHERE id=$1 FOR UPDATE")
            .bind(supplier_id)
            .fetch_optional(&mut *tx)
            .await?;
    let owner_id = owner_id.ok_or_else(AppError::missing)?;

    let is_holder = actor.id == consumer_id;
    let is_owner = actor.id == owner_id;
    if !is_holder && !is_owner && !actor.moderator() {
        return Err(AppError::forbidden());
    }

    lock.acquire(Authority::Offer, &offer_id.to_string())?;
    let offer: Option<Offer> =
        sqlx::query_as("SELECT * FROM suasobra.offers WHERE id=$1 FOR UPDATE")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let offer = offer.ok_or_else(AppError::missing)?;

    lock.acquire(Authority::Reservation, &reservation_id.to_string())?;
    let reservation_row: Option<(String, i64, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT status, quantity, accepted_policy_version, accepted_pickup_ends_at FROM suasobra.reservations WHERE id=$1 FOR UPDATE",
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, quantity, policy_version, accepted_pickup_ends_at) =
        reservation_row.ok_or_else(AppError::missing)?;

    // Terminal protection (INV-007): exactly one release per quantity.
    if status != "confirmed" {
        return Err(AppError::conflict(format!(
            "Only confirmed reservations can be cancelled; current status is '{status}'"
        )));
    }

    // Exclusive pickup deadline (P03-05): once the accepted pickup window has
    // ended the reservation is due for expiry, not cancellation — even if the
    // maintenance timer has not run yet.
    if Utc::now() >= accepted_pickup_ends_at {
        return Err(AppError::conflict(
            "The pickup window has ended; the reservation is due for expiry and can no longer be cancelled",
        ));
    }

    let new_available = offer
        .available_quantity
        .checked_add(quantity)
        .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?;
    let new_committed = offer
        .committed_quantity
        .checked_sub(quantity)
        .ok_or_else(AppError::internal)?;
    let prior_val: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM suasobra.reservations r WHERE id=$1")
            .bind(reservation_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "UPDATE suasobra.offers SET available_quantity=$2, committed_quantity=$3, updated_at=now() WHERE id=$1",
    )
    .bind(offer_id)
    .bind(new_available)
    .bind(new_committed)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE suasobra.reservations SET status='cancelled' WHERE id=$1")
        .bind(reservation_id)
        .execute(&mut *tx)
        .await?;

    let resulting_val: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM suasobra.reservations r WHERE id=$1")
            .bind(reservation_id)
            .fetch_one(&mut *tx)
            .await?;

    let mut fact = AuditFact {
        entity_type: "reservation".into(),
        entity_id: reservation_id.to_string(),
        action: "reservation_cancelled".into(),
        actor_id: Some(actor.id),
        actor_scope: if is_holder {
            "consumer"
        } else if is_owner {
            "supplier_owner"
        } else {
            "moderator"
        }
        .into(),
        authority_or_policy: policy_version,
        evidence_kind: "participant_acknowledgment".into(),
        reason,
        prior_state: prior_val,
        resulting_state: resulting_val.clone(),
        prior_quantity: Some(offer.available_quantity),
        resulting_quantity: Some(new_available),
        quantity_delta: Some(quantity),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;

    replay.finish(&mut tx, 200, &resulting_val).await?;
    tx.commit().await?;
    Ok((StatusCode::OK, resulting_val))
}

/// Outcome of one repeatable expiry run, for timer logs and tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpiryReport {
    pub expired_reservations: u64,
    pub expired_offers: u64,
    pub units_disposed: u64,
}

/// Expire due reservations and offers in one atomic transaction.
///
/// A host timer may invoke this through the `expire-due` executable command;
/// no worker or broker exists. Anything with an ended pickup window
/// (`accepted_pickup_ends_at <= now` for reservations, `pickup_ends_at <= now`
/// for published/scheduled offers) transitions to `expired` with its exact
/// remaining units moved to `disposed` (INV-007): past-pickup goods are never
/// re-sold, and expiry never moves more than the accepted quantity (INV-008).
/// Each transition records an audit fact. The run is repeatable: a second run
/// finds nothing due and changes nothing; a crash rolls the whole batch back
/// so conserved quantities and accepted history survive intact.
pub async fn expire_due(pool: &PgPool, now: DateTime<Utc>) -> Result<ExpiryReport> {
    let mut tx = pool.begin().await?;
    let mut report = ExpiryReport {
        expired_reservations: 0,
        expired_offers: 0,
        units_disposed: 0,
    };

    // Offers before reservations (authority order), each set in deterministic
    // ID order so concurrent runs cannot deadlock. Every row is re-read and
    // re-validated under its lock: a concurrent withdraw/cancel that commits
    // first makes the row simply skipped instead of double-disposed.
    let due_offers: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM suasobra.offers WHERE status IN ('published','scheduled') AND pickup_ends_at IS NOT NULL AND pickup_ends_at <= $1 ORDER BY id FOR UPDATE",
    )
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    for offer_id in due_offers {
        let fresh: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT status, available_quantity, policy_version FROM suasobra.offers WHERE id=$1 FOR UPDATE",
        )
        .bind(offer_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, available, policy_version)) = fresh else {
            continue;
        };
        if status != "published" && status != "scheduled" {
            continue;
        }
        let new_available = 0;
        let disposed_add = available;
        sqlx::query(
            "UPDATE suasobra.offers SET status='expired', available_quantity=$2, disposed_quantity=disposed_quantity+$3, updated_at=now() WHERE id=$1",
        )
        .bind(offer_id)
        .bind(new_available)
        .bind(disposed_add)
        .execute(&mut *tx)
        .await?;
        let offer_val: Value =
            sqlx::query_scalar("SELECT to_jsonb(o) FROM suasobra.offers o WHERE id=$1")
                .bind(offer_id)
                .fetch_one(&mut *tx)
                .await?;
        let mut fact = AuditFact {
            entity_type: "offer".into(),
            entity_id: offer_id.to_string(),
            action: "offer_expired".into(),
            actor_id: None,
            actor_scope: "system".into(),
            authority_or_policy: policy_version,
            evidence_kind: "system_maintenance".into(),
            reason: "Pickup window ended without sale; remaining available units disposed".into(),
            prior_state: json!({}),
            resulting_state: offer_val,
            prior_quantity: Some(available),
            resulting_quantity: Some(new_available),
            quantity_delta: Some(-disposed_add),
            ..Default::default()
        };
        fact.record(&mut tx, None).await?;
        report.expired_offers += 1;
        report.units_disposed += disposed_add as u64;
    }

    let due_reservations: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM suasobra.reservations WHERE status='confirmed' AND accepted_pickup_ends_at <= $1 ORDER BY id FOR UPDATE",
    )
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    for reservation_id in due_reservations {
        let fresh: Option<(Uuid, String, i64, String)> = sqlx::query_as(
            "SELECT offer_id, status, quantity, accepted_policy_version FROM suasobra.reservations WHERE id=$1 FOR UPDATE",
        )
        .bind(reservation_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((offer_id, status, quantity, policy_version)) = fresh else {
            continue;
        };
        if status != "confirmed" {
            continue;
        }
        let committed: Option<i64> = sqlx::query_scalar(
            "SELECT committed_quantity FROM suasobra.offers WHERE id=$1 FOR UPDATE",
        )
        .bind(offer_id)
        .fetch_optional(&mut *tx)
        .await?;
        let committed = committed.ok_or_else(AppError::missing)?;
        let new_committed = committed
            .checked_sub(quantity)
            .ok_or_else(AppError::internal)?;
        sqlx::query(
            "UPDATE suasobra.offers SET committed_quantity=$2, disposed_quantity=disposed_quantity+$3, updated_at=now() WHERE id=$1",
        )
        .bind(offer_id)
        .bind(new_committed)
        .bind(quantity)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE suasobra.reservations SET status='expired' WHERE id=$1")
            .bind(reservation_id)
            .execute(&mut *tx)
            .await?;
        let resulting_val: Value =
            sqlx::query_scalar("SELECT to_jsonb(r) FROM suasobra.reservations r WHERE id=$1")
                .bind(reservation_id)
                .fetch_one(&mut *tx)
                .await?;
        let mut fact = AuditFact {
            entity_type: "reservation".into(),
            entity_id: reservation_id.to_string(),
            action: "reservation_expired".into(),
            actor_id: None,
            actor_scope: "system".into(),
            authority_or_policy: policy_version,
            evidence_kind: "system_maintenance".into(),
            reason: "Pickup window ended without handover; committed units disposed".into(),
            prior_state: json!({}),
            resulting_state: resulting_val,
            prior_quantity: Some(committed),
            resulting_quantity: Some(new_committed),
            quantity_delta: Some(-quantity),
            ..Default::default()
        };
        fact.record(&mut tx, None).await?;
        report.expired_reservations += 1;
        report.units_disposed += quantity as u64;
    }

    tx.commit().await?;
    Ok(report)
}

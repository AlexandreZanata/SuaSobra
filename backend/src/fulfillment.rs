use crate::{
    AppState,
    audit::AuditFact,
    error::{AppError, Result},
    identity::{self, User},
    primitives::{Authority, LockOrder},
    replay::Replay,
};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Typed collector scope: the reservation holder collecting in person, or a
/// single nominated third party. No advanced representative management exists
/// in launch; nomination names exactly one existing user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectorScope {
    Holder,
    Nominated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePickupProofInput {
    #[serde(default)]
    pub reservation_id: Option<Uuid>,
    #[serde(default)]
    pub collector_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletePickupInput {
    #[serde(default)]
    pub reservation_id: Option<Uuid>,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletePartialPickupInput {
    #[serde(default)]
    pub reservation_id: Option<Uuid>,
    pub secret: String,
    pub handover_quantity: i64,
    /// Explicit terminal outcome for the uncollected remainder: `released`
    /// returns genuine surplus to the offer for re-offer, `disposed` moves it
    /// to the disposed bucket (unsafe, perished, or otherwise unusable).
    pub remainder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    IssuePickupProof(IssuePickupProofInput),
    CompletePickup(CompletePickupInput),
    CompletePartialPickup(CompletePartialPickupInput),
}

/// Minimal handover data revealed by a valid proof: who to expect, how much,
/// and in what condition. Contact details and addresses are never included:
/// the collector already holds the exact location from their reservation
/// detail, and the owner already knows their own address (INV-057).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedPickup {
    pub proof_id: Uuid,
    pub reservation_id: Uuid,
    pub collector_id: Uuid,
    pub collector_first_name: String,
    pub scope: CollectorScope,
    pub quantity: i64,
    pub condition: String,
}

/// Issue a hashed expiring single-use pickup proof for a confirmed reservation.
///
/// Only the reservation holder may issue: the entitlement — and therefore the
/// authority to name its collector — is theirs. The 32-byte secret is shown
/// exactly once in the response; only its SHA-256 hash is persisted, bound to
/// the reservation with the accepted pickup window as its expiry. A bare
/// reservation UUID never authorizes pickup.
pub async fn execute(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    command: Command,
) -> Result<(StatusCode, Value)> {
    if let Command::CompletePickup(input) = command {
        return execute_pickup(state, actor, idempotency_key, input).await;
    }
    if let Command::CompletePartialPickup(input) = command {
        return execute_partial_pickup(state, actor, idempotency_key, input).await;
    }
    let Command::IssuePickupProof(input) = command else {
        return Err(AppError::internal());
    };
    let reservation_id = input.reservation_id.ok_or_else(|| {
        AppError::validation("reservation_id is required to issue a pickup proof")
    })?;

    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;

    let mut lock = LockOrder::default();
    lock.acquire(Authority::Consumer, &actor.id.to_string())?;

    let command_json = serde_json::to_value(&input).map_err(|_| AppError::internal())?;
    let (replay, cached) = Replay::begin(
        &mut tx,
        actor.id,
        "issue_pickup_proof",
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

    lock.acquire(Authority::Reservation, &reservation_id.to_string())?;
    let row: Option<(Uuid, Uuid, String, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT consumer_id, supplier_id, status, accepted_pickup_ends_at, accepted_policy_version FROM suasobra.reservations WHERE id=$1 FOR UPDATE",
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (consumer_id, supplier_id, status, pickup_ends_at, policy_version) =
        row.ok_or_else(AppError::missing)?;

    if actor.id != consumer_id {
        return Err(AppError::forbidden());
    }
    if status != "confirmed" {
        return Err(AppError::conflict(format!(
            "Pickup proofs require a confirmed reservation; current status is '{status}'"
        )));
    }
    if Utc::now() >= pickup_ends_at {
        return Err(AppError::conflict(
            "The pickup window has ended; no new pickup proof can be issued",
        ));
    }

    let collector_id = input.collector_id.unwrap_or(consumer_id);
    let scope = if collector_id == consumer_id {
        CollectorScope::Holder
    } else {
        CollectorScope::Nominated
    };
    if scope == CollectorScope::Nominated {
        let nominee: Option<(bool, bool, String)> = sqlx::query_as(
            "SELECT is_active, declared_adult, standing FROM suasobra.users WHERE id=$1",
        )
        .bind(collector_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (active, adult, standing) =
            nominee.ok_or_else(|| AppError::validation("Nominated collector does not exist"))?;
        if !active || !adult || ["suspended", "blocked", "closed"].contains(&standing.as_str()) {
            return Err(AppError::validation(
                "Nominated collector must be an active adult account in good standing",
            ));
        }
        let owner_id: Option<Uuid> =
            sqlx::query_scalar("SELECT owner_id FROM suasobra.suppliers WHERE id=$1")
                .bind(supplier_id)
                .fetch_optional(&mut *tx)
                .await?;
        if owner_id.is_some_and(|id| id == collector_id) {
            return Err(AppError::validation(
                "The supplier cannot be nominated to collect its own offer",
            ));
        }
    }

    let secret = identity::secret()?;
    let digest = identity::hash(&secret);
    let scope_text = match scope {
        CollectorScope::Holder => "holder",
        CollectorScope::Nominated => "nominated",
    };
    let proof_id: Uuid = sqlx::query_scalar(
        "INSERT INTO suasobra.pickup_proofs (reservation_id, collector_id, scope, proof_hash, expires_at, created_by) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(reservation_id)
    .bind(collector_id)
    .bind(scope_text)
    .bind(digest)
    .bind(pickup_ends_at)
    .bind(actor.id)
    .fetch_one(&mut *tx)
    .await?;
    let proof_val: Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM suasobra.pickup_proofs p WHERE id=$1")
            .bind(proof_id)
            .fetch_one(&mut *tx)
            .await?;

    let mut fact = AuditFact {
        entity_type: "pickup_proof".into(),
        entity_id: proof_id.to_string(),
        action: "pickup_proof_issued".into(),
        actor_id: Some(actor.id),
        actor_scope: "consumer".into(),
        authority_or_policy: policy_version,
        evidence_kind: "participant_acknowledgment".into(),
        reason: format!(
            "Pickup proof issued for {} collector {}",
            if scope == CollectorScope::Holder {
                "holder"
            } else {
                "nominated"
            },
            collector_id
        ),
        prior_state: json!({}),
        resulting_state: proof_val.clone(),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;

    // The secret is shown exactly once, here; replay returns this same sealed
    // response rather than minting a second proof (INV-061).
    let response = json!({
        "id": proof_id,
        "reservation_id": reservation_id,
        "collector_id": collector_id,
        "scope": scope,
        "secret": secret,
        "expires_at": pickup_ends_at,
    });
    replay.finish(&mut tx, 201, &response).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, response))
}

/// Verify that a presented secret is the live proof for a reservation.
///
/// Returns the minimal handover data on success. Wrong, expired, already
/// used, or cross-reservation secrets all fail with one undifferentiated
/// error so proofs are not an oracle (INV-057). Verification never mutates:
/// redemption happens atomically inside the handover command (P04-02).
pub async fn verify_pickup_proof(
    pool: &PgPool,
    reservation_id: Uuid,
    secret: &str,
) -> Result<VerifiedPickup> {
    #[derive(sqlx::FromRow)]
    struct ProofRow {
        proof_id: Uuid,
        collector_id: Uuid,
        collector_first_name: String,
        scope: Value,
        quantity: i64,
        condition: String,
    }

    let row: Option<ProofRow> = sqlx::query_as(
        r#"
        SELECT p.id AS proof_id,
               p.collector_id AS collector_id,
               u.first_name AS collector_first_name,
               to_jsonb(p.scope) AS scope,
               r.quantity AS quantity,
               r.accepted_condition AS condition
        FROM suasobra.pickup_proofs p
        JOIN suasobra.users u ON u.id = p.collector_id
        JOIN suasobra.reservations r ON r.id = p.reservation_id
        WHERE p.reservation_id = $1
          AND p.proof_hash = $2
          AND p.used_at IS NULL
          AND p.expires_at > now()
        "#,
    )
    .bind(reservation_id)
    .bind(identity::hash(secret))
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| AppError::validation("Invalid or expired pickup proof"))?;
    let scope: CollectorScope =
        serde_json::from_value(row.scope).map_err(|_| AppError::internal())?;
    Ok(VerifiedPickup {
        proof_id: row.proof_id,
        reservation_id,
        collector_id: row.collector_id,
        collector_first_name: row.collector_first_name,
        scope,
        quantity: row.quantity,
        condition: row.condition,
    })
}

/// Atomically redeem a live proof for the handover command.
///
/// The single `UPDATE ... WHERE used_at IS NULL` marks exactly one redemption:
/// concurrent redemptions of the same secret yield one success (INV-007).
/// Takes the handover transaction so proof and quantity move together (P04-02).
pub async fn consume_pickup_proof(
    tx: &mut Transaction<'_, Postgres>,
    reservation_id: Uuid,
    secret: &str,
) -> Result<VerifiedPickup> {
    // The single conditional UPDATE is the atomic redemption: exactly one
    // concurrent consumer can flip used_at from NULL (INV-007).
    let row: Option<(Uuid, Uuid, Value)> = sqlx::query_as(
        "UPDATE suasobra.pickup_proofs SET used_at = now() WHERE reservation_id = $1 AND proof_hash = $2 AND used_at IS NULL AND expires_at > now() RETURNING id, collector_id, to_jsonb(scope)",
    )
    .bind(reservation_id)
    .bind(identity::hash(secret))
    .fetch_optional(&mut **tx)
    .await?;

    let (proof_id, collector_id, scope_val) =
        row.ok_or_else(|| AppError::validation("Invalid or expired pickup proof"))?;
    let scope: CollectorScope =
        serde_json::from_value(scope_val).map_err(|_| AppError::internal())?;
    let detail: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT u.first_name, r.quantity, r.accepted_condition FROM suasobra.users u, suasobra.reservations r WHERE u.id = $1 AND r.id = $2",
    )
    .bind(collector_id)
    .bind(reservation_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (collector_first_name, quantity, condition) = detail.ok_or_else(AppError::internal)?;
    Ok(VerifiedPickup {
        proof_id,
        reservation_id,
        collector_id,
        collector_first_name,
        scope,
        quantity,
        condition,
    })
}

/// Complete one final full pickup of a confirmed reservation.
///
/// The caller must be the supplier owner (or a moderator) AND present the
/// collector's live proof secret: neither owner authority alone nor a bare
/// reservation UUID completes anything, so unknown clients cannot forge
/// completion. Proof, quantity and status move in one transaction — the proof
/// is redeemed, the full accepted quantity moves committed→fulfilled, and the
/// reservation becomes `picked_up`. Any second handover finds a used proof
/// and a terminal status, so one reservation yields one final pickup, one
/// unit effect and one event (INV-007, INV-009).
///
/// The accepted price is recorded as the known goods value handed over; the
/// platform verifies no financial receipt and settles nothing (ADR-002).
async fn execute_pickup(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    input: CompletePickupInput,
) -> Result<(StatusCode, Value)> {
    let reservation_id = input
        .reservation_id
        .ok_or_else(|| AppError::validation("reservation_id is required to complete pickup"))?;
    if input.secret.trim().is_empty() {
        return Err(AppError::validation("Pickup proof secret is required"));
    }

    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;

    let mut lock = LockOrder::default();
    lock.acquire(Authority::Consumer, &actor.id.to_string())?;

    let command_json = serde_json::to_value(&input).map_err(|_| AppError::internal())?;
    let (replay, cached) = Replay::begin(
        &mut tx,
        actor.id,
        "complete_pickup",
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

    let linkage: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT offer_id, consumer_id FROM suasobra.reservations WHERE id=$1")
            .bind(reservation_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (offer_id, _consumer_id) = linkage.ok_or_else(AppError::missing)?;

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

    // Handover authority: the supplier owner (or a moderator) verifies the
    // collector in person. Consumers, nominees and outsiders cannot complete,
    // even holding a valid secret.
    if actor.id != owner_id && !actor.moderator() {
        return Err(AppError::forbidden());
    }

    lock.acquire(Authority::Offer, &offer_id.to_string())?;
    let offer: Option<crate::marketplace::Offer> =
        sqlx::query_as("SELECT * FROM suasobra.offers WHERE id=$1 FOR UPDATE")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let offer = offer.ok_or_else(AppError::missing)?;

    lock.acquire(Authority::Reservation, &reservation_id.to_string())?;
    let reservation_row: Option<(String, i64, i64, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT status, quantity, accepted_price_cents, accepted_condition, accepted_policy_version, accepted_pickup_ends_at FROM suasobra.reservations WHERE id=$1 FOR UPDATE",
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, quantity, price_cents, condition, policy_version, pickup_ends_at) =
        reservation_row.ok_or_else(AppError::missing)?;

    if status != "confirmed" {
        return Err(AppError::conflict(format!(
            "Only confirmed reservations can be picked up; current status is '{status}'"
        )));
    }
    if Utc::now() >= pickup_ends_at {
        return Err(AppError::conflict(
            "The pickup window has ended; handover is no longer possible",
        ));
    }

    // Redeem the collector's proof in this same transaction: verification
    // without actual handover is a separate read-only primitive and consumes
    // nothing, while this redemption moves with the goods.
    let verified = consume_pickup_proof(&mut tx, reservation_id, &input.secret).await?;
    if verified.quantity != quantity || verified.condition != condition {
        return Err(AppError::internal());
    }

    let new_committed = offer
        .committed_quantity
        .checked_sub(quantity)
        .ok_or_else(AppError::internal)?;
    let new_fulfilled = offer
        .fulfilled_quantity
        .checked_add(quantity)
        .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?;
    sqlx::query(
        "UPDATE suasobra.offers SET committed_quantity=$2, fulfilled_quantity=$3, updated_at=now() WHERE id=$1",
    )
    .bind(offer_id)
    .bind(new_committed)
    .bind(new_fulfilled)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE suasobra.reservations SET status='picked_up' WHERE id=$1")
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
        action: "pickup_completed".into(),
        actor_id: Some(actor.id),
        actor_scope: if actor.id == owner_id {
            "supplier_owner"
        } else {
            "moderator"
        }
        .into(),
        authority_or_policy: policy_version,
        evidence_kind: "participant_acknowledgment".into(),
        reason: format!(
            "Handover of {quantity} unit(s) to {} collector {} acknowledged; accepted goods value recorded without financial receipt",
            if verified.scope == CollectorScope::Holder {
                "holder"
            } else {
                "nominated"
            },
            verified.collector_id
        ),
        prior_state: json!({}),
        resulting_state: resulting_val.clone(),
        prior_quantity: Some(offer.committed_quantity),
        resulting_quantity: Some(new_committed),
        quantity_delta: Some(-quantity),
        money_delta_cents: Some(
            price_cents
                .checked_mul(quantity)
                .ok_or_else(|| AppError::validation("Goods value overflow"))?,
        ),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;

    replay.finish(&mut tx, 200, &resulting_val).await?;
    tx.commit().await?;
    Ok((StatusCode::OK, resulting_val))
}

/// Complete one final partial handover of whole separable units.
///
/// Only reservations of more than one unit can split: each sellable unit is
/// indivisible (INV-010), so a single-unit reservation must use the full
/// handover, and the handed-over count must be a positive whole-unit amount
/// strictly below the accepted quantity. The uncollected remainder reaches an
/// explicit terminal outcome in the same transaction (INV-011): `released`
/// returns genuine surplus to the offer for re-offer, `disposed` moves
/// perished or unsafe units to the disposed bucket. Either way the
/// reservation becomes `picked_up` and no later command can revive
/// handed-over inventory (INV-007, INV-009, INV-014).
///
/// The accepted unit price stays fixed and only the handed-over units are
/// recorded as goods value; no refund is processed (ADR-002).
async fn execute_partial_pickup(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    input: CompletePartialPickupInput,
) -> Result<(StatusCode, Value)> {
    let reservation_id = input
        .reservation_id
        .ok_or_else(|| AppError::validation("reservation_id is required to complete pickup"))?;
    if input.secret.trim().is_empty() {
        return Err(AppError::validation("Pickup proof secret is required"));
    }
    if input.handover_quantity < 1 {
        return Err(AppError::validation(
            "handover_quantity must be a positive whole-unit count",
        ));
    }
    let release_remainder = match input.remainder.as_str() {
        "released" => true,
        "disposed" => false,
        _ => {
            return Err(AppError::validation(
                "remainder must be 'released' or 'disposed'",
            ));
        }
    };

    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;

    let mut lock = LockOrder::default();
    lock.acquire(Authority::Consumer, &actor.id.to_string())?;

    let command_json = serde_json::to_value(&input).map_err(|_| AppError::internal())?;
    let (replay, cached) = Replay::begin(
        &mut tx,
        actor.id,
        "complete_partial_pickup",
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

    let linkage: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT offer_id, consumer_id FROM suasobra.reservations WHERE id=$1")
            .bind(reservation_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (offer_id, _consumer_id) = linkage.ok_or_else(AppError::missing)?;

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

    if actor.id != owner_id && !actor.moderator() {
        return Err(AppError::forbidden());
    }

    lock.acquire(Authority::Offer, &offer_id.to_string())?;
    let offer: Option<crate::marketplace::Offer> =
        sqlx::query_as("SELECT * FROM suasobra.offers WHERE id=$1 FOR UPDATE")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
    let offer = offer.ok_or_else(AppError::missing)?;

    lock.acquire(Authority::Reservation, &reservation_id.to_string())?;
    let reservation_row: Option<(String, i64, i64, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT status, quantity, accepted_price_cents, accepted_condition, accepted_policy_version, accepted_pickup_ends_at FROM suasobra.reservations WHERE id=$1 FOR UPDATE",
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, quantity, price_cents, condition, policy_version, pickup_ends_at) =
        reservation_row.ok_or_else(AppError::missing)?;

    if status != "confirmed" {
        return Err(AppError::conflict(format!(
            "Only confirmed reservations can be picked up; current status is '{status}'"
        )));
    }
    if Utc::now() >= pickup_ends_at {
        return Err(AppError::conflict(
            "The pickup window has ended; handover is no longer possible",
        ));
    }

    // Whole-unit split only: a single-unit reservation is indivisible and a
    // handover at or above the accepted quantity belongs to the full command.
    if input.handover_quantity >= quantity {
        return Err(AppError::validation(
            "handover_quantity must be below the accepted quantity; use the full handover for the whole reservation",
        ));
    }
    let remainder = quantity - input.handover_quantity;

    let verified = consume_pickup_proof(&mut tx, reservation_id, &input.secret).await?;
    if verified.quantity != quantity || verified.condition != condition {
        return Err(AppError::internal());
    }

    let new_committed = offer
        .committed_quantity
        .checked_sub(quantity)
        .ok_or_else(AppError::internal)?;
    let new_fulfilled = offer
        .fulfilled_quantity
        .checked_add(input.handover_quantity)
        .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?;
    let (new_available, new_disposed) = if release_remainder {
        (
            offer
                .available_quantity
                .checked_add(remainder)
                .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?,
            offer.disposed_quantity,
        )
    } else {
        (
            offer.available_quantity,
            offer
                .disposed_quantity
                .checked_add(remainder)
                .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?,
        )
    };
    sqlx::query(
        "UPDATE suasobra.offers SET committed_quantity=$2, fulfilled_quantity=$3, available_quantity=$4, disposed_quantity=$5, updated_at=now() WHERE id=$1",
    )
    .bind(offer_id)
    .bind(new_committed)
    .bind(new_fulfilled)
    .bind(new_available)
    .bind(new_disposed)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE suasobra.reservations SET status='picked_up' WHERE id=$1")
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
        action: "partial_pickup_completed".into(),
        actor_id: Some(actor.id),
        actor_scope: if actor.id == owner_id {
            "supplier_owner"
        } else {
            "moderator"
        }
        .into(),
        authority_or_policy: policy_version,
        evidence_kind: "participant_acknowledgment".into(),
        reason: format!(
            "Partial handover of {} of {quantity} unit(s) to {} collector {} acknowledged; remainder {remainder} unit(s) {}; accepted goods value recorded without financial receipt",
            input.handover_quantity,
            if verified.scope == CollectorScope::Holder {
                "holder"
            } else {
                "nominated"
            },
            verified.collector_id,
            if release_remainder {
                "released for re-offer"
            } else {
                "disposed"
            },
        ),
        prior_state: json!({}),
        resulting_state: resulting_val.clone(),
        prior_quantity: Some(offer.committed_quantity),
        resulting_quantity: Some(new_committed),
        quantity_delta: Some(-quantity),
        money_delta_cents: Some(
            price_cents
                .checked_mul(input.handover_quantity)
                .ok_or_else(|| AppError::validation("Goods value overflow"))?,
        ),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;

    replay.finish(&mut tx, 200, &resulting_val).await?;
    tx.commit().await?;
    Ok((StatusCode::OK, resulting_val))
}

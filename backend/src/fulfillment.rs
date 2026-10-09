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
#[serde(tag = "type", content = "payload")]
pub enum Command {
    IssuePickupProof(IssuePickupProofInput),
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
    let Command::IssuePickupProof(input) = command;
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

use crate::{
    error::{AppError, Result},
    replay,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub fn redact(value: &Value) -> (Value, Vec<String>) {
    fn walk(value: &Value, path: &str, paths: &mut Vec<String>) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, value)| {
                        let next = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{path}.{key}")
                        };
                        let lower = key.to_lowercase();
                        let sensitive = [
                            "password",
                            "passwd",
                            "secret",
                            "token",
                            "auth",
                            "credentials",
                            "card_number",
                            "cvv",
                            "cvc",
                            "security_code",
                            "pix_key",
                            "bank_account",
                            "routing_number",
                            "credit_card",
                            "phone",
                            "cpf",
                            "tax_id",
                        ];
                        let clean = if sensitive.iter().any(|part| lower.contains(part)) {
                            paths.push(next);
                            json!("[REDACTED]")
                        } else {
                            walk(value, &next, paths)
                        };
                        (key.clone(), clean)
                    })
                    .collect(),
            ),
            Value::Array(list) => Value::Array(
                list.iter()
                    .enumerate()
                    .map(|(i, v)| walk(v, &format!("{path}[{i}]"), paths))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    let mut paths = vec![];
    let clean = walk(value, "", &mut paths);
    paths.sort();
    (clean, paths)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditFact {
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub actor_id: Option<Uuid>,
    pub actor_scope: String,
    pub authority_or_policy: String,
    pub evidence_kind: String,
    pub reason: String,
    pub prior_state: Value,
    pub resulting_state: Value,
    pub prior_quantity: Option<i64>,
    pub resulting_quantity: Option<i64>,
    pub quantity_delta: Option<i64>,
    pub money_delta_cents: Option<i64>,
    pub payload: Value,
    pub metadata: Value,
    pub correction_of: Option<Uuid>,
    pub idempotency_key: Option<String>,
}
impl Default for AuditFact {
    fn default() -> Self {
        Self {
            entity_type: String::new(),
            entity_id: String::new(),
            action: String::new(),
            actor_id: None,
            actor_scope: String::new(),
            authority_or_policy: String::new(),
            evidence_kind: "participant_acknowledgment".into(),
            reason: String::new(),
            prior_state: json!({}),
            resulting_state: json!({}),
            prior_quantity: None,
            resulting_quantity: None,
            quantity_delta: None,
            money_delta_cents: None,
            payload: json!({}),
            metadata: json!({}),
            correction_of: None,
            idempotency_key: None,
        }
    }
}
impl AuditFact {
    pub fn validate(&mut self) -> Result<()> {
        if self.entity_type.trim().is_empty()
            || self.entity_id.trim().is_empty()
            || self.action.trim().is_empty()
            || self.authority_or_policy.trim().is_empty()
        {
            return Err(AppError::validation(
                "Audit entity, action and authority are mandatory",
            ));
        }
        if self.correction_of.is_some()
            && (self.reason.trim().is_empty()
                || (self.actor_id.is_none() && self.actor_scope.trim().is_empty()))
        {
            return Err(AppError::validation(
                "A correction requires actor and reason",
            ));
        }
        if self.prior_quantity.is_some()
            || self.resulting_quantity.is_some()
            || self.quantity_delta.is_some()
        {
            let (Some(prior), Some(resulting)) = (self.prior_quantity, self.resulting_quantity)
            else {
                return Err(AppError::validation("Both quantities are required"));
            };
            if prior < 0
                || resulting < 0
                || self.reason.trim().is_empty()
                || (self.actor_id.is_none() && self.actor_scope.trim().is_empty())
            {
                return Err(AppError::validation(
                    "Quantity adjustment requires nonnegative units, actor and reason",
                ));
            }
            let delta = resulting
                .checked_sub(prior)
                .ok_or_else(|| AppError::validation("Quantity overflow"))?;
            if self.quantity_delta.is_some_and(|given| given != delta) {
                return Err(AppError::validation("Quantity delta does not match"));
            }
            self.quantity_delta = Some(delta);
        }
        Ok(())
    }
    pub async fn record(
        &mut self,
        tx: &mut Transaction<'_, Postgres>,
        outbox_topic: Option<&str>,
    ) -> Result<Uuid> {
        self.validate()?;
        if let Some(key) = &self.idempotency_key {
            replay::lock(tx, &format!("audit:{key}")).await?;
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM suasobra.audit_facts WHERE idempotency_key=$1")
                    .bind(key)
                    .fetch_optional(&mut **tx)
                    .await?;
            if let Some(id) = existing {
                return Ok(id);
            }
        }
        let (payload, redacted_fields) = redact(&self.payload);
        let id:Uuid=sqlx::query_scalar("INSERT INTO suasobra.audit_facts(entity_type,entity_id,action,actor_id,actor_scope,authority_or_policy,evidence_kind,reason,prior_state,resulting_state,prior_quantity,resulting_quantity,quantity_delta,money_delta_cents,payload,metadata,redacted_fields,correction_of,idempotency_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) RETURNING id")
            .bind(&self.entity_type).bind(&self.entity_id).bind(&self.action).bind(self.actor_id).bind(&self.actor_scope).bind(&self.authority_or_policy)
            .bind(&self.evidence_kind).bind(&self.reason).bind(redact(&self.prior_state).0).bind(redact(&self.resulting_state).0)
            .bind(self.prior_quantity).bind(self.resulting_quantity).bind(self.quantity_delta).bind(self.money_delta_cents)
            .bind(payload.clone()).bind(redact(&self.metadata).0).bind(json!(redacted_fields)).bind(self.correction_of).bind(&self.idempotency_key)
            .fetch_one(&mut **tx).await?;
        if let Some(topic) = outbox_topic {
            enqueue(
                tx,
                topic,
                &self.action,
                &payload,
                &json!({"audit_fact_id":id}),
                self.idempotency_key
                    .as_ref()
                    .map(|k| format!("{k}:outbox"))
                    .as_deref(),
            )
            .await?;
        }
        Ok(id)
    }
}
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    topic: &str,
    event: &str,
    payload: &Value,
    headers: &Value,
    key: Option<&str>,
) -> Result<Uuid> {
    if topic.trim().is_empty() || event.trim().is_empty() {
        return Err(AppError::validation("Outbox topic and event are mandatory"));
    }
    if let Some(key) = key {
        replay::lock(tx, &format!("outbox:{key}")).await?;
        if let Some(id) =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM suasobra.outbox WHERE idempotency_key=$1")
                .bind(key)
                .fetch_optional(&mut **tx)
                .await?
        {
            return Ok(id);
        }
    }
    Ok(sqlx::query_scalar("INSERT INTO suasobra.outbox(topic,event_type,payload,headers,idempotency_key) VALUES($1,$2,$3,$4,$5) RETURNING id")
        .bind(topic).bind(event).bind(redact(payload).0).bind(redact(headers).0).bind(key).fetch_one(&mut **tx).await?)
}
pub async fn correction(
    tx: &mut Transaction<'_, Postgres>,
    original: Uuid,
    actor: Uuid,
    reason: &str,
    state: Value,
    adjustment: Option<i64>,
) -> Result<Uuid> {
    let row: Value =
        sqlx::query_scalar("SELECT to_jsonb(f) FROM suasobra.audit_facts f WHERE id=$1")
            .bind(original)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(AppError::missing)?;
    let prior = row["resulting_quantity"].as_i64();
    let resulting = match adjustment {
        Some(delta) => Some(
            prior
                .unwrap_or(0)
                .checked_add(delta)
                .ok_or_else(|| AppError::validation("Quantity overflow"))?,
        ),
        None => prior,
    };
    let mut fact = AuditFact {
        entity_type: row["entity_type"].as_str().unwrap_or_default().into(),
        entity_id: row["entity_id"].as_str().unwrap_or_default().into(),
        action: format!("{}_corrected", row["action"].as_str().unwrap_or_default()),
        actor_id: Some(actor),
        reason: reason.into(),
        authority_or_policy: row["authority_or_policy"]
            .as_str()
            .unwrap_or_default()
            .into(),
        prior_state: row["resulting_state"].clone(),
        resulting_state: state,
        prior_quantity: prior,
        resulting_quantity: resulting,
        quantity_delta: adjustment,
        correction_of: Some(original),
        ..Default::default()
    };
    fact.record(tx, None).await
}

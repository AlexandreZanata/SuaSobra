use crate::error::{AppError, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub fn digest(value: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("JSON values serialize"),
    ))
}
pub async fn lock(tx: &mut Transaction<'_, Postgres>, key: &str) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub struct Replay {
    actor: Uuid,
    command: String,
    key: Option<String>,
    digest: String,
}
impl Replay {
    pub async fn begin(
        tx: &mut Transaction<'_, Postgres>,
        actor: Uuid,
        command: &str,
        key: Option<&str>,
        input: &Value,
    ) -> Result<(Self, Option<(u16, Value)>)> {
        if key.is_some_and(|k| k.trim().is_empty() || k.len() > 128) {
            return Err(AppError::validation("Invalid Idempotency-Key"));
        }
        let this = Self {
            actor,
            command: command.into(),
            key: key.map(str::to_owned),
            digest: digest(input),
        };
        if let Some(key) = &this.key {
            lock(tx, &format!("replay:{actor}:{command}:{key}")).await?;
            let row:Option<(String,i32,Value)>=sqlx::query_as("SELECT input_digest,result_code,response_payload FROM suasobra.command_replays WHERE actor_id=$1 AND command_name=$2 AND idempotency_key=$3")
                .bind(actor).bind(command).bind(key).fetch_optional(&mut **tx).await?;
            if let Some((digest, code, payload)) = row {
                if digest != this.digest {
                    return Err(AppError::conflict(
                        "Idempotency key was used with different input",
                    ));
                }
                return Ok((this, Some((code as u16, payload))));
            }
        }
        Ok((this, None))
    }
    pub async fn finish(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        code: u16,
        payload: &Value,
    ) -> Result<()> {
        if let Some(key) = &self.key {
            // Access-checked responses may contain owner contact data. Never log/replay credentials.
            sqlx::query("INSERT INTO suasobra.command_replays(actor_id,command_name,idempotency_key,input_digest,result_code,response_payload) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(self.actor).bind(&self.command).bind(key).bind(&self.digest).bind(i32::from(code)).bind(payload)
                .execute(&mut **tx).await?;
        }
        Ok(())
    }
}

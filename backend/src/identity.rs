use crate::{
    AppState,
    error::{AppError, Result},
    replay,
};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::http::{HeaderMap, StatusCode};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::num::NonZeroU32;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub fn secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AppError::internal())?;
    Ok(hex::encode(bytes))
}
pub fn hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|s| s.split(';'))
        .filter_map(|s| s.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub phone_number: String,
    pub is_email_verified: bool,
    pub is_phone_verified: bool,
    pub declared_adult: bool,
    pub is_active: bool,
    pub standing: String,
    pub restriction: String,
    pub roles: Value,
}
impl User {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles
            .as_array()
            .is_some_and(|roles| roles.iter().any(|r| r.as_str() == Some(role)))
            || (role == "consumer" && self.roles.as_array().is_some_and(Vec::is_empty))
    }
    pub fn moderator(&self) -> bool {
        self.is_active
            && self.standing == "active"
            && self.restriction != "complete_suspension"
            && self.has_role("moderator")
    }
    pub fn can_publish(&self) -> bool {
        self.is_active
            && self.standing == "active"
            && !["publication_restricted", "complete_suspension"]
                .contains(&self.restriction.as_str())
            && self.has_role("supplier_owner")
    }
    pub fn can_reserve(&self, supplier_owner: Uuid) -> bool {
        self.id != supplier_owner
            && self.is_active
            && self.declared_adult
            && !["suspended", "blocked", "closed"].contains(&self.standing.as_str())
            && !["reservation_restricted", "complete_suspension"]
                .contains(&self.restriction.as_str())
            && self.has_role("consumer")
    }
    pub fn can_access_remedy(&self, participant: Uuid) -> bool {
        self.id == participant || self.moderator()
    }
    pub fn summary(&self) -> Value {
        let mut v = serde_json::to_value(self).expect("User serializes");
        v.as_object_mut()
            .expect("User is object")
            .remove("is_active");
        v["username"] = json!(self.email);
        if v["roles"] == json!([]) {
            v["roles"] = json!(["consumer"]);
        }
        v
    }
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub csrf_hash: String,
    pub expires_at: DateTime<Utc>,
}
pub async fn session(state: &AppState, headers: &HeaderMap) -> Result<Option<Session>> {
    let Some(token) = cookie(headers, "sessionid") else {
        return Ok(None);
    };
    if token.len() != 64 {
        return Ok(None);
    }
    Ok(sqlx::query_as("SELECT id,user_id,csrf_hash,expires_at FROM suasobra.sessions WHERE token_hash=$1 AND expires_at>now()")
        .bind(hash(token)).fetch_optional(&state.pool).await?)
}
pub async fn user(state: &AppState, session: Option<&Session>) -> Result<User> {
    let id = session
        .and_then(|s| s.user_id)
        .ok_or_else(AppError::unauthenticated)?;
    sqlx::query_as("SELECT id,email,first_name,phone_number,is_email_verified,is_phone_verified,declared_adult,is_active,standing,restriction,roles FROM suasobra.users WHERE id=$1 AND is_active")
        .bind(id).fetch_optional(&state.pool).await?.ok_or_else(AppError::unauthenticated)
}
pub async fn lock_actor(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<User> {
    sqlx::query_as("SELECT id,email,first_name,phone_number,is_email_verified,is_phone_verified,declared_adult,is_active,standing,restriction,roles FROM suasobra.users WHERE id=$1 AND is_active FOR UPDATE")
        .bind(id).fetch_optional(&mut **tx).await?.ok_or_else(AppError::unauthenticated)
}
pub fn csrf(session: Option<&Session>, headers: &HeaderMap) -> Result<()> {
    let session = session.ok_or_else(AppError::forbidden)?;
    let token = headers
        .get("x-csrftoken")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if token.len() != 64
        || hash(token)
            .as_bytes()
            .ct_eq(session.csrf_hash.as_bytes())
            .unwrap_u8()
            != 1
    {
        return Err(AppError::forbidden());
    }
    Ok(())
}
pub fn cookies(token: &str, csrf: &str, secure: bool) -> Vec<String> {
    let secure = if secure { "; Secure" } else { "" };
    vec![
        format!("sessionid={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400{secure}"),
        format!("csrftoken={csrf}; Path=/; SameSite=Lax; Max-Age=86400{secure}"),
    ]
}
pub async fn rotate(
    tx: &mut Transaction<'_, Postgres>,
    old: Option<&Session>,
    user: Option<Uuid>,
    secure: bool,
) -> Result<(String, Vec<String>)> {
    let token = secret()?;
    let csrf = secret()?;
    if let Some(old) = old {
        sqlx::query("DELETE FROM suasobra.sessions WHERE id=$1")
            .bind(old.id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("INSERT INTO suasobra.sessions(token_hash,csrf_hash,user_id,expires_at) VALUES($1,$2,$3,now()+interval '24 hours')")
        .bind(hash(&token)).bind(hash(&csrf)).bind(user).execute(&mut **tx).await?;
    Ok((csrf.clone(), cookies(&token, &csrf, secure)))
}
pub async fn hash_password(state: &AppState, password: String) -> Result<String> {
    let slot = state
        .password_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_exceeded",
                "Authentication is busy. Try again shortly.",
            )
        })?;
    tokio::task::spawn_blocking(move || {
        let _slot = slot;
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|_| AppError::internal())
    })
    .await
    .map_err(|_| AppError::internal())?
}
pub fn verify_password(password: &str, encoded: &str) -> bool {
    if encoded.starts_with("pbkdf2_sha256$") || encoded.starts_with("pbkdf2_sha1$") {
        let parts: Vec<_> = encoded.split('$').collect();
        if parts.len() != 4 {
            return false;
        }
        let Some(iterations) = parts[1]
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= 10_000_000)
            .and_then(NonZeroU32::new)
        else {
            return false;
        };
        let Ok(expected) = STANDARD.decode(parts[3]) else {
            return false;
        };
        let algorithm = if parts[0] == "pbkdf2_sha256" {
            ring::pbkdf2::PBKDF2_HMAC_SHA256
        } else {
            ring::pbkdf2::PBKDF2_HMAC_SHA1
        };
        return ring::pbkdf2::verify(
            algorithm,
            iterations,
            parts[2].as_bytes(),
            password.as_bytes(),
            &expected,
        )
        .is_ok();
    }
    let encoded = encoded
        .strip_prefix("argon2$")
        .map(|s| format!("${s}"))
        .unwrap_or_else(|| encoded.to_owned());
    PasswordHash::new(&encoded).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}
pub fn email(value: &str) -> Result<String> {
    let value = value.trim().to_lowercase();
    let Some((local, domain)) = value.split_once('@') else {
        return Err(AppError::validation("Invalid email"));
    };
    let local_valid = !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c));
    let domain_valid = domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        });
    if value.len() > 254 || !local_valid || !domain_valid {
        return Err(AppError::validation("Invalid email"));
    }
    Ok(value)
}
pub fn password_valid(value: &str) -> Result<()> {
    if !(8..=128).contains(&value.chars().count()) {
        return Err(AppError::validation("Password must have 8..128 characters"));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub email: String,
    pub password: String,
    pub declared_adult: bool,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub phone_number: String,
}
pub async fn register(
    state: &AppState,
    session: Option<&Session>,
    body: Value,
) -> Result<(u16, Value, Vec<String>)> {
    let data: Registration = serde_json::from_value(body)
        .map_err(|_| AppError::validation("Invalid registration fields"))?;
    let email = email(&data.email)?;
    password_valid(&data.password)?;
    if !data.declared_adult || data.first_name.chars().count() > 150 || data.phone_number.len() > 32
    {
        return Err(AppError::validation(
            "Adult declaration and valid profile fields are required",
        ));
    }
    let password = hash_password(state, data.password).await?;
    let mut tx = state.pool.begin().await?;
    let id:Uuid=sqlx::query_scalar("INSERT INTO suasobra.users(email,password,first_name,phone_number,declared_adult) VALUES($1,$2,$3,$4,true) RETURNING id")
        .bind(email).bind(password).bind(data.first_name.trim()).bind(data.phone_number.trim()).fetch_one(&mut *tx).await?;
    let user = lock_actor(&mut tx, id).await?;
    let (_, cookies) = rotate(&mut tx, session, Some(id), state.config.production).await?;
    tx.commit().await?;
    Ok((201, user.summary(), cookies))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    email: String,
    password: String,
}
pub async fn login(
    state: &AppState,
    session: Option<&Session>,
    body: Value,
    ip: &str,
) -> Result<(u16, Value, Vec<String>)> {
    let data: Login =
        serde_json::from_value(body).map_err(|_| AppError::validation("Invalid login fields"))?;
    if data.email.len() > 254 || data.password.len() > 512 {
        return Err(AppError::validation("Invalid credentials"));
    }
    let email = data.email.trim().to_lowercase();
    let mut tx = state.pool.begin().await?;
    // Shared PostgreSQL locks serialize identifier/IP counters across backend instances.
    let mut locks = [format!("login:email:{email}"), format!("login:ip:{ip}")];
    locks.sort();
    for key in locks {
        replay::lock(&mut tx, &key).await?;
    }
    let limited:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM suasobra.login_attempts WHERE NOT successful AND attempt_time>=now()-interval '15 minutes' AND (identifier=$1 OR ip_address=$2::inet) GROUP BY identifier HAVING count(*)>=5) OR (SELECT count(*)>=5 FROM suasobra.login_attempts WHERE NOT successful AND attempt_time>=now()-interval '15 minutes' AND ip_address=$2::inet)")
        .bind(&email).bind(ip).fetch_one(&mut *tx).await?;
    if limited {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_exceeded",
            "Too many failed login attempts. Try again later.",
        ));
    }
    let row: Option<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id,password,is_active FROM suasobra.users WHERE lower(email)=$1 FOR UPDATE",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    let encoded = row.as_ref().map(|r| r.1.clone()).unwrap_or_default();
    let slot = state
        .password_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_exceeded",
                "Authentication is busy.",
            )
        })?;
    let password = data.password;
    let valid = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        if encoded.is_empty() {
            let _ = Argon2::default().hash_password(password.as_bytes());
            false
        } else {
            verify_password(&password, &encoded)
        }
    })
    .await
    .map_err(|_| AppError::internal())?
        && row.as_ref().is_some_and(|r| r.2);
    sqlx::query("INSERT INTO suasobra.login_attempts(identifier,ip_address,successful) VALUES($1,$2::inet,$3)").bind(email).bind(ip).bind(valid).execute(&mut *tx).await?;
    if !valid {
        tx.commit().await?;
        return Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "authentication_failed",
            "Invalid email or password.",
        ));
    }
    let id = row.ok_or_else(AppError::internal)?.0;
    let user = lock_actor(&mut tx, id).await?;
    let (_, cookies) = rotate(&mut tx, session, Some(id), state.config.production).await?;
    tx.commit().await?;
    Ok((200, user.summary(), cookies))
}
/// Retained token primitive; automatic delivery endpoints are deferred by ADR-002.
pub async fn issue_token(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    purpose: &str,
) -> Result<String> {
    let ttl = match purpose {
        "email_verification" => 86400i64,
        "password_reset" => 3600,
        _ => return Err(AppError::validation("Invalid token purpose")),
    };
    let token = secret()?;
    sqlx::query("INSERT INTO suasobra.identity_tokens(user_id,token_hash,purpose,expires_at) VALUES($1,$2,$3,now()+make_interval(secs=>$4))")
        .bind(user).bind(hash(&token)).bind(purpose).bind(ttl as f64).execute(&mut **tx).await?;
    Ok(token)
}
pub async fn consume_token(
    tx: &mut Transaction<'_, Postgres>,
    token: &str,
    purpose: &str,
) -> Result<Uuid> {
    sqlx::query_scalar("UPDATE suasobra.identity_tokens SET used_at=now() WHERE token_hash=$1 AND purpose=$2 AND used_at IS NULL AND expires_at>now() RETURNING user_id")
        .bind(hash(token)).bind(purpose).fetch_optional(&mut **tx).await?.ok_or_else(||AppError::validation("Invalid or expired token"))
}

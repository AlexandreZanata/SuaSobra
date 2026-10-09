use crate::{
    AppState,
    audit::AuditFact,
    error::{AppError, Result},
    identity::{self, User},
    replay::Replay,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    trade_name: String,
    tax_id: String,
    contact_email: String,
    contact_phone: String,
    #[serde(default)]
    legal_name: String,
    #[serde(default = "other")]
    business_type: String,
}
fn other() -> String {
    "other".into()
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    correction_notes: String,
    #[serde(default)]
    reapplication_conditions: String,
    #[serde(default)]
    review_date: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishmentInput {
    name: String,
    #[serde(default)]
    description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceInterval {
    day_of_week: u8,
    start_time: String,
    end_time: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointData {
    pub name: String,
    pub address_street: String,
    pub address_number: String,
    #[serde(default)]
    pub address_complement: String,
    pub address_neighborhood: String,
    pub address_city: String,
    pub address_state: String,
    pub address_postal_code: String,
    pub latitude: Value,
    pub longitude: Value,
    #[serde(default = "default_timezone")]
    pub time_zone: String,
    #[serde(default)]
    pub access_instructions: String,
    #[serde(default)]
    pub service_intervals: Vec<ServiceInterval>,
    #[serde(default)]
    pub is_private_address: bool,
    #[serde(skip_deserializing, default)]
    pub approx_latitude: String,
    #[serde(skip_deserializing, default)]
    pub approx_longitude: String,
}
fn default_timezone() -> String {
    "America/Sao_Paulo".into()
}
fn required(value: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(AppError::validation(
            "A required field is empty or too long",
        ));
    }
    Ok(value.into())
}
fn decimal(value: &Value, limit: i64) -> Result<(String, String)> {
    let raw = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err(AppError::validation("Coordinates must be decimal values")),
    };
    let negative = raw.starts_with('-');
    let magnitude = raw.trim_start_matches('-');
    let (whole, fraction) = magnitude.split_once('.').unwrap_or((magnitude, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(AppError::validation(
            "Coordinates must have at most six decimal places",
        ));
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| AppError::validation("Invalid coordinates"))?;
    let padded = format!("{fraction:0<6}");
    let fraction = padded
        .parse::<i64>()
        .map_err(|_| AppError::validation("Invalid coordinates"))?;
    let units = whole
        .checked_mul(1_000_000)
        .and_then(|w| w.checked_add(fraction))
        .ok_or_else(|| AppError::validation("Invalid coordinates"))?;
    if units > limit * 1_000_000 {
        return Err(AppError::validation(
            "Coordinates are outside the valid range",
        ));
    }
    let sign = if negative && units != 0 { "-" } else { "" };
    // Decimal round-half-to-even matches the previous Python Decimal behavior.
    let hundredths = units / 10_000;
    let remainder = units % 10_000;
    let rounded =
        hundredths + i64::from(remainder > 5000 || (remainder == 5000 && hundredths % 2 == 1));
    let approx_sign = if negative && rounded != 0 { "-" } else { "" };
    Ok((
        format!("{sign}{}.{:06}", units / 1_000_000, units % 1_000_000),
        format!("{approx_sign}{}.{:02}", rounded / 100, rounded % 100),
    ))
}
fn valid_time(value: &str) -> bool {
    value.is_ascii()
        && value.len() == 5
        && value.as_bytes()[2] == b':'
        && value[0..2].parse::<u8>().is_ok_and(|h| h < 24)
        && value[3..5].parse::<u8>().is_ok_and(|m| m < 60)
}
impl PointData {
    pub fn validate(&mut self) -> Result<()> {
        self.name = required(&self.name, 255)?;
        self.address_street = required(&self.address_street, 255)?;
        self.address_number = required(&self.address_number, 32)?;
        self.address_neighborhood = required(&self.address_neighborhood, 128)?;
        self.address_city = required(&self.address_city, 128)?;
        self.address_state = required(&self.address_state, 2)?.to_uppercase();
        if self.address_state.len() != 2
            || !self.address_state.bytes().all(|c| c.is_ascii_alphabetic())
        {
            return Err(AppError::validation("State must have two letters"));
        }
        self.address_postal_code = required(&self.address_postal_code, 16)?;
        if self.address_complement.chars().count() > 255 || self.access_instructions.len() > 4000 {
            return Err(AppError::validation("Address details are too long"));
        }
        self.time_zone = self.time_zone.trim().into();
        self.time_zone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| AppError::validation("Invalid IANA time zone"))?;
        let (lat, approx_lat) = decimal(&self.latitude, 90)?;
        let (lng, approx_lng) = decimal(&self.longitude, 180)?;
        self.latitude = json!(lat);
        self.longitude = json!(lng);
        self.approx_latitude = approx_lat;
        self.approx_longitude = approx_lng;
        for interval in &self.service_intervals {
            if interval.day_of_week > 6
                || !valid_time(&interval.start_time)
                || !valid_time(&interval.end_time)
                || interval.start_time >= interval.end_time
            {
                return Err(AppError::validation(
                    "Invalid service interval; use Sunday=0..Saturday=6 and start<end",
                ));
            }
        }
        Ok(())
    }
}
pub enum Command {
    Declare(Declaration),
    Transition(Uuid, String, Transition),
    Establishment(Uuid, EstablishmentInput),
    CreatePoint(Uuid, Box<PointData>),
    UpdatePoint(Uuid, Value),
}
impl Command {
    pub fn parse(name: &str, id: Option<Uuid>, body: Value) -> Result<Self> {
        let malformed = |_| AppError::validation("Invalid or unknown command fields");
        let id_required = || id.ok_or_else(AppError::missing);
        Ok(match name {
            "declare" => Self::Declare(serde_json::from_value(body).map_err(malformed)?),
            "establishment" => Self::Establishment(
                id_required()?,
                serde_json::from_value(body).map_err(malformed)?,
            ),
            "create_point" => Self::CreatePoint(
                id_required()?,
                serde_json::from_value(body).map_err(malformed)?,
            ),
            "update_point" => Self::UpdatePoint(id_required()?, body),
            action
                if [
                    "approve",
                    "reject",
                    "reapply",
                    "warn",
                    "clear-warning",
                    "suspend",
                    "close",
                ]
                .contains(&action) =>
            {
                Self::Transition(
                    id_required()?,
                    action.into(),
                    serde_json::from_value(body).map_err(malformed)?,
                )
            }
            _ => return Err(AppError::missing()),
        })
    }
}
pub async fn supplier(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(s) FROM suasobra.suppliers s WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::missing)
}
fn owner_id(supplier: &Value) -> Result<Uuid> {
    supplier["owner_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(AppError::internal)
}
pub fn access(actor: &User, supplier: &Value) -> Result<()> {
    if owner_id(supplier)? != actor.id && !actor.moderator() {
        return Err(AppError::forbidden());
    }
    Ok(())
}
fn open(supplier: &Value) -> Result<()> {
    if supplier["standing"] == "closed" {
        return Err(AppError::validation("Supplier is closed"));
    }
    Ok(())
}
async fn event(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    actor: Uuid,
    event: &str,
    states: (&str, &str),
    reason: &str,
    evidence: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO suasobra.supplier_events(supplier_id,actor_id,event_type,prior_standing,resulting_standing,reason,evidence) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(actor).bind(event).bind(states.0).bind(states.1).bind(reason).bind(evidence).execute(&mut **tx).await?;
    Ok(())
}
async fn point_context(tx: &mut Transaction<'_, Postgres>, point: Uuid) -> Result<(Value, Uuid)> {
    let establishment: Uuid =
        sqlx::query_scalar("SELECT establishment_id FROM suasobra.pickup_points WHERE id=$1")
            .bind(point)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(AppError::missing)?;
    establishment_context(tx, establishment).await
}
async fn establishment_context(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(Value, Uuid)> {
    let supplier_id: Uuid =
        sqlx::query_scalar("SELECT supplier_id FROM suasobra.establishments WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(AppError::missing)?;
    Ok((supplier(tx, supplier_id).await?, id))
}
/// All HTTP/operations entry points share this explicit transaction boundary.
pub async fn execute(
    state: &AppState,
    actor: Uuid,
    name: &str,
    id: Option<Uuid>,
    key: Option<&str>,
    input: Value,
) -> Result<(u16, Value)> {
    let command = Command::parse(name, id, input.clone())?;
    let mut tx = state.pool.begin().await?;
    // Refresh permissions inside the transaction and lock account before supplier authority.
    let actor = identity::lock_actor(&mut tx, actor).await?;
    match &command {
        Command::Declare(_) => {
            if !actor.declared_adult
                || actor.standing != "active"
                || actor.restriction == "complete_suspension"
            {
                return Err(AppError::forbidden());
            }
        }
        Command::Transition(id, action, _) => {
            let s = supplier(&mut tx, *id).await?;
            if ["approve", "reject", "warn", "clear-warning", "suspend"].contains(&action.as_str())
            {
                if !actor.moderator() {
                    return Err(AppError::forbidden());
                }
            } else if action == "reapply" {
                if owner_id(&s)? != actor.id {
                    return Err(AppError::forbidden());
                }
            } else {
                access(&actor, &s)?;
            }
        }
        Command::Establishment(id, _) => {
            access(&actor, &supplier(&mut tx, *id).await?)?;
        }
        Command::CreatePoint(id, _) => {
            let (s, _) = establishment_context(&mut tx, *id).await?;
            access(&actor, &s)?;
        }
        Command::UpdatePoint(id, _) => {
            let (s, _) = point_context(&mut tx, *id).await?;
            access(&actor, &s)?;
        }
    }
    let command_name = format!("{name}:{}", id.map(|id| id.to_string()).unwrap_or_default());
    let (replay, existing) = Replay::begin(&mut tx, actor.id, &command_name, key, &input).await?;
    if let Some(result) = existing {
        tx.commit().await?;
        return Ok(result);
    }
    let (code, result, entity, entity_id, prior, reason) = match command {
        Command::Declare(data) => {
            let trade = required(&data.trade_name, 255)?;
            let tax = required(&data.tax_id, 32)?;
            let email = identity::email(&data.contact_email)?;
            let phone = required(&data.contact_phone, 32)?;
            if data.legal_name.chars().count() > 255
                || !["bakery", "restaurant", "grocery", "cafe", "hotel", "other"]
                    .contains(&data.business_type.as_str())
            {
                return Err(AppError::validation("Invalid business details"));
            }
            let id:Uuid=sqlx::query_scalar("INSERT INTO suasobra.suppliers(owner_id,trade_name,legal_name,tax_id,business_type,contact_email,contact_phone) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
                .bind(actor.id).bind(trade).bind(data.legal_name.trim()).bind(tax).bind(data.business_type).bind(email).bind(phone).fetch_one(&mut *tx).await?;
            let mut roles = actor.roles.as_array().cloned().unwrap_or_default();
            if roles.is_empty() {
                roles.push(json!("consumer"));
            }
            if !roles.contains(&json!("supplier_owner")) {
                roles.push(json!("supplier_owner"));
            }
            sqlx::query("UPDATE suasobra.users SET roles=$1 WHERE id=$2")
                .bind(json!(roles))
                .bind(actor.id)
                .execute(&mut *tx)
                .await?;
            event(
                &mut tx,
                id,
                actor.id,
                "supplier_registered",
                ("", "pending_approval"),
                "Supplier registration submitted for operational review.",
                "",
            )
            .await?;
            (
                201,
                supplier(&mut tx, id).await?,
                "supplier",
                id,
                json!({}),
                "Supplier registration submitted.".into(),
            )
        }
        Command::Transition(id, action, data) => {
            let prior = supplier(&mut tx, id).await?;
            let standing = prior["standing"].as_str().ok_or_else(AppError::internal)?;
            let next = match (action.as_str(), standing) {
                ("approve", "pending_approval") => "active",
                ("reject", "pending_approval") => "rejected",
                ("reapply", "rejected") => "pending_approval",
                ("warn", "active") => "warned",
                ("clear-warning", "warned") => "active",
                ("suspend", s) if s != "closed" => "suspended",
                ("close", s) if s != "closed" => "closed",
                _ => return Err(AppError::conflict("Invalid supplier standing transition")),
            };
            let reason = if action == "reapply" {
                if data.correction_notes.trim().is_empty() {
                    "Supplier resubmitted application for review.".into()
                } else {
                    data.correction_notes.trim().into()
                }
            } else if action == "approve" && data.reason.trim().is_empty() {
                "Checks passed.".into()
            } else if action == "clear-warning" && data.reason.trim().is_empty() {
                "Correction verified.".into()
            } else {
                required(&data.reason, 4000)?
            };
            sqlx::query("UPDATE suasobra.suppliers SET standing=$2,updated_at=now(),rejection_reason=CASE WHEN $3='reject' THEN $4 WHEN $3='approve' THEN '' ELSE rejection_reason END,reapplication_conditions=CASE WHEN $3='reject' THEN $5 WHEN $3='approve' THEN '' ELSE reapplication_conditions END,warning_reason=CASE WHEN $3='warn' THEN $4 WHEN $3='clear-warning' THEN '' ELSE warning_reason END,suspension_reason=CASE WHEN $3='suspend' THEN $4 ELSE suspension_reason END,suspension_review_date=CASE WHEN $3='suspend' THEN $6 ELSE suspension_review_date END,closure_reason=CASE WHEN $3='close' THEN $4 ELSE closure_reason END,closure_date=CASE WHEN $3='close' THEN now() ELSE closure_date END WHERE id=$1")
                .bind(id).bind(next).bind(&action).bind(&reason).bind(data.reapplication_conditions.trim()).bind(data.review_date).execute(&mut *tx).await?;
            if action == "approve" || action == "reject" {
                event(
                    &mut tx,
                    id,
                    actor.id,
                    if action == "approve" {
                        "supplier_approved"
                    } else {
                        "supplier_rejected"
                    },
                    (standing, next),
                    &reason,
                    &data.reapplication_conditions,
                )
                .await?;
            }
            event(
                &mut tx,
                id,
                actor.id,
                "supplier_standing_changed",
                (standing, next),
                &reason,
                "",
            )
            .await?;
            (
                200,
                supplier(&mut tx, id).await?,
                "supplier",
                id,
                prior,
                reason,
            )
        }
        Command::Establishment(id, data) => {
            open(&supplier(&mut tx, id).await?)?;
            let name = required(&data.name, 255)?;
            let result:Value=sqlx::query_scalar("INSERT INTO suasobra.establishments(supplier_id,name,description) VALUES($1,$2,$3) RETURNING to_jsonb(establishments)")
                .bind(id).bind(name).bind(data.description.trim()).fetch_one(&mut *tx).await?;
            let id = result["id"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .ok_or_else(AppError::internal)?;
            (
                201,
                result,
                "establishment",
                id,
                json!({}),
                "Initial creation.".into(),
            )
        }
        Command::CreatePoint(establishment, mut data) => {
            let (s, _) = establishment_context(&mut tx, establishment).await?;
            open(&s)?;
            data.validate()?;
            let id:Uuid=sqlx::query_scalar("INSERT INTO suasobra.pickup_points(establishment_id,data) VALUES($1,$2) RETURNING id")
                .bind(establishment).bind(json!(data)).fetch_one(&mut *tx).await?;
            let result = point_full(&mut tx, id).await?;
            history(&mut tx, id, 1, actor.id, &result, "Initial creation.").await?;
            (
                201,
                result,
                "pickup_point",
                id,
                json!({}),
                "Initial creation.".into(),
            )
        }
        Command::UpdatePoint(id, input) => {
            let (s, _) = point_context(&mut tx, id).await?;
            open(&s)?;
            let prior = point_full(&mut tx, id).await?;
            let mut patch = input
                .as_object()
                .cloned()
                .ok_or_else(|| AppError::validation("Expected a JSON object"))?;
            let reason = patch
                .remove("change_reason")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(|| AppError::validation("Change reason is required"))?;
            let reason = required(&reason, 4000)?;
            if patch.contains_key("latitude") != patch.contains_key("longitude") {
                return Err(AppError::validation("Provide both coordinates"));
            }
            let active = match patch.remove("is_active") {
                Some(Value::Bool(v)) => v,
                Some(_) => return Err(AppError::validation("is_active must be boolean")),
                None => prior["is_active"].as_bool().unwrap_or(true),
            };
            let current: Value = sqlx::query_scalar(
                "SELECT data FROM suasobra.pickup_points WHERE id=$1 FOR UPDATE",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let mut data = current
                .as_object()
                .cloned()
                .ok_or_else(AppError::internal)?;
            data.remove("approx_latitude");
            data.remove("approx_longitude");
            for (k, v) in patch {
                if !data.contains_key(&k) {
                    return Err(AppError::validation("Unknown pickup point field"));
                }
                data.insert(k, v);
            }
            let mut data: PointData = serde_json::from_value(Value::Object(data))
                .map_err(|_| AppError::validation("Invalid pickup point fields"))?;
            data.validate()?;
            let version:i32=sqlx::query_scalar("UPDATE suasobra.pickup_points SET data=$2,version=version+1,is_active=$3,updated_at=now() WHERE id=$1 RETURNING version")
                .bind(id).bind(json!(data)).bind(active).fetch_one(&mut *tx).await?;
            let result = point_full(&mut tx, id).await?;
            history(&mut tx, id, version, actor.id, &result, &reason).await?;
            (200, result, "pickup_point", id, prior, reason)
        }
    };
    let mut fact = AuditFact {
        entity_type: entity.into(),
        entity_id: entity_id.to_string(),
        action: name.into(),
        actor_id: Some(actor.id),
        actor_scope: if actor.moderator() {
            "moderator"
        } else {
            "supplier_owner"
        }
        .into(),
        authority_or_policy: "2026.2-free-launch".into(),
        reason,
        prior_state: prior,
        resulting_state: result.clone(),
        ..Default::default()
    };
    fact.record(&mut tx, None).await?;
    replay.finish(&mut tx, code, &result).await?;
    tx.commit().await?;
    Ok((code, result))
}
async fn point_full(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT p.data || (to_jsonb(p)-'data') || jsonb_build_object('supplier_id',e.supplier_id,'establishment_name',e.name) FROM suasobra.pickup_points p JOIN suasobra.establishments e ON e.id=p.establishment_id WHERE p.id=$1")
        .bind(id).fetch_one(&mut **tx).await.map_err(Into::into)
}
async fn history(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: i32,
    actor: Uuid,
    snapshot: &Value,
    reason: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO suasobra.pickup_point_history(pickup_point_id,version,actor_id,snapshot,change_reason) VALUES($1,$2,$3,$4,$5)")
        .bind(id).bind(version).bind(actor).bind(snapshot).bind(reason).execute(&mut **tx).await?;
    Ok(())
}
pub fn public_point(mut point: Value) -> Value {
    if point["is_private_address"] == true {
        point["address_street"] = json!("[Private location revealed after reservation]");
        point["address_number"] = json!("");
    }
    for key in [
        "latitude",
        "longitude",
        "access_instructions",
        "address_complement",
        "created_at",
        "updated_at",
        "establishment_id",
    ] {
        point
            .as_object_mut()
            .expect("Point is an object")
            .remove(key);
    }
    point
}
pub async fn query(state: &AppState, actor: &User, kind: &str, id: Option<Uuid>) -> Result<Value> {
    let mut tx = state.pool.begin().await?;
    let result = match kind {
        "suppliers" => {
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(s) FROM suasobra.suppliers s WHERE owner_id=$1 OR $2 ORDER BY created_at DESC,id DESC LIMIT 100")
            .bind(actor.id).bind(actor.moderator()).fetch_all(&mut *tx).await?;
            json!(rows)
        }
        "supplier" => {
            let s = supplier(&mut tx, id.ok_or_else(AppError::missing)?).await?;
            access(actor, &s)?;
            s
        }
        "events" | "establishments" => {
            let id = id.ok_or_else(AppError::missing)?;
            access(actor, &supplier(&mut tx, id).await?)?;
            let sql = if kind == "events" {
                "SELECT to_jsonb(e)||jsonb_build_object('actor_email',u.email) FROM suasobra.supplier_events e LEFT JOIN suasobra.users u ON e.actor_id=u.id WHERE supplier_id=$1 ORDER BY e.created_at DESC,e.id DESC LIMIT 100"
            } else {
                "SELECT to_jsonb(e) FROM suasobra.establishments e WHERE supplier_id=$1 ORDER BY id LIMIT 100"
            };
            json!(
                sqlx::query_scalar::<_, Value>(sql)
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?
            )
        }
        "points" => {
            let id = id.ok_or_else(AppError::missing)?;
            let (s, _) = establishment_context(&mut tx, id).await?;
            let full = access(actor, &s).is_ok();
            let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM suasobra.pickup_points WHERE establishment_id=$1 AND is_active ORDER BY id LIMIT 100").bind(id).fetch_all(&mut *tx).await?;
            let mut list = vec![];
            for id in ids {
                let p = point_full(&mut tx, id).await?;
                list.push(if full { p } else { public_point(p) });
            }
            json!(list)
        }
        "point" | "history" => {
            let id = id.ok_or_else(AppError::missing)?;
            let (s, _) = point_context(&mut tx, id).await?;
            if kind == "history" {
                access(actor, &s)?;
                json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(h)||jsonb_build_object('actor_email',u.email) FROM suasobra.pickup_point_history h LEFT JOIN suasobra.users u ON h.actor_id=u.id WHERE pickup_point_id=$1 ORDER BY version DESC LIMIT 100").bind(id).fetch_all(&mut *tx).await?)
            } else {
                let p = point_full(&mut tx, id).await?;
                if access(actor, &s).is_ok() {
                    p
                } else {
                    public_point(p)
                }
            }
        }
        _ => return Err(AppError::missing()),
    };
    tx.commit().await?;
    Ok(result)
}

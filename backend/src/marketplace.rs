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
use sqlx::PgPool;
use uuid::Uuid;

pub const CANONICAL_CATEGORIES: &[&str] = &[
    "food",
    "fruit_vegetables",
    "construction",
    "wood_carpentry",
    "paper_printing",
    "packaging",
    "agriculture",
    "workshop_parts",
    "textiles_offcuts",
    "other_reusable",
];

pub const FOOD_CONDITIONS: &[&str] = &[
    "prepared_today",
    "prepared_previously",
    "near_expiry",
    "shelf_stable",
    "surprise_assortment",
];

pub const NON_FOOD_CONDITIONS: &[&str] = &[
    "fresh",
    "surplus_production",
    "cosmetic_defect",
    "imperfect_packaging",
    "day_end_clearance",
    "clean_offcuts",
    "usable_surplus",
    "intact",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurplusBuckets {
    pub admitted: u64,
    pub available: u64,
    pub committed: u64,
    pub fulfilled: u64,
    pub disposed: u64,
}

impl SurplusBuckets {
    pub fn new(
        admitted: u64,
        available: u64,
        committed: u64,
        fulfilled: u64,
        disposed: u64,
    ) -> Result<Self> {
        let buckets = Self {
            admitted,
            available,
            committed,
            fulfilled,
            disposed,
        };
        buckets.validate()?;
        Ok(buckets)
    }

    pub fn initial(admitted: u64) -> Result<Self> {
        if admitted == 0 {
            return Err(AppError::validation("Admitted quantity must be positive"));
        }
        Self::new(admitted, admitted, 0, 0, 0)
    }

    pub fn validate(&self) -> Result<()> {
        let sum = self
            .available
            .checked_add(self.committed)
            .and_then(|s| s.checked_add(self.fulfilled))
            .and_then(|s| s.checked_add(self.disposed))
            .ok_or_else(|| AppError::validation("Bucket quantity overflow"))?;
        if sum != self.admitted {
            return Err(AppError::validation(format!(
                "Unbalanced buckets: admitted ({}) does not equal available ({}) + committed ({}) + fulfilled ({}) + disposed ({})",
                self.admitted, self.available, self.committed, self.fulfilled, self.disposed
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Offer {
    pub id: Uuid,
    pub supplier_id: Uuid,
    pub establishment_id: Uuid,
    pub pickup_point_id: Uuid,
    pub title: String,
    pub description: String,
    pub category: String,
    pub unit: String,
    pub status: String,
    pub admitted_quantity: i64,
    pub available_quantity: i64,
    pub committed_quantity: i64,
    pub fulfilled_quantity: i64,
    pub disposed_quantity: i64,
    pub policy_version: String,
    pub surplus_reason: String,
    pub condition: String,
    pub safety_disclosures: Value,
    pub price_cents: i64,
    pub currency: String,
    pub booking_starts_at: Option<DateTime<Utc>>,
    pub booking_ends_at: Option<DateTime<Utc>>,
    pub pickup_starts_at: Option<DateTime<Utc>>,
    pub pickup_ends_at: Option<DateTime<Utc>>,
    pub payment_mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Offer {
    pub fn buckets(&self) -> SurplusBuckets {
        SurplusBuckets {
            admitted: self.admitted_quantity as u64,
            available: self.available_quantity as u64,
            committed: self.committed_quantity as u64,
            fulfilled: self.fulfilled_quantity as u64,
            disposed: self.disposed_quantity as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SafetyDisclosures {
    #[serde(default)]
    pub storage_instructions: Option<String>,
    #[serde(default)]
    pub age_or_preparation: Option<String>,
    #[serde(default)]
    pub expiry_or_best_before: Option<DateTime<Utc>>,
    #[serde(default)]
    pub allergens_or_dietary: Option<String>,
    #[serde(default)]
    pub safe_handling: Option<String>,
    #[serde(default)]
    pub condition_notes: Option<String>,
    #[serde(default)]
    pub packaging_integrity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDraftInput {
    pub supplier_id: Uuid,
    pub establishment_id: Uuid,
    pub pickup_point_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub subcategory: Option<String>,
    pub unit: String,
    pub admitted_quantity: u64,
    pub surplus_reason: String,
    pub condition: String,
    pub safety_disclosures: SafetyDisclosures,
    #[serde(default)]
    pub price_cents: Option<u64>,
}

impl CreateDraftInput {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > 255 {
            return Err(AppError::validation(
                "Title must be between 1 and 255 characters",
            ));
        }

        let unit = self.unit.trim();
        if unit.is_empty() || unit.chars().count() > 64 {
            return Err(AppError::validation(
                "Unit must be between 1 and 64 characters",
            ));
        }

        if self.admitted_quantity == 0 {
            return Err(AppError::validation("Admitted quantity must be positive"));
        }

        let category = self.category.trim();
        if !CANONICAL_CATEGORIES.contains(&category) {
            return Err(AppError::validation(format!(
                "Category must be one of the canonical categories: {:?}",
                CANONICAL_CATEGORIES
            )));
        }

        let surplus_reason = self.surplus_reason.trim();
        if surplus_reason.len() < 5 {
            return Err(AppError::validation(
                "A substantiated surplus classification/reason is required (at least 5 characters)",
            ));
        }

        let condition = self.condition.trim();
        if category == "food" {
            if !FOOD_CONDITIONS.contains(&condition) {
                return Err(AppError::validation(format!(
                    "Food condition must be one of {:?}; unsafe, spoiled or expired goods cannot be offered",
                    FOOD_CONDITIONS
                )));
            }

            // Material food safety disclosures (INV-063, INV-017)
            let storage = self
                .safety_disclosures
                .storage_instructions
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if storage.is_empty() {
                return Err(AppError::validation(
                    "Food disclosures cannot conceal storage needs: storage_instructions is required",
                ));
            }

            let age = self
                .safety_disclosures
                .age_or_preparation
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if age.is_empty() {
                return Err(AppError::validation(
                    "Food disclosures cannot conceal age or preparation: age_or_preparation is required",
                ));
            }

            let allergens = self
                .safety_disclosures
                .allergens_or_dietary
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if allergens.is_empty() {
                return Err(AppError::validation(
                    "Food disclosures cannot conceal allergens or dietary risks: allergens_or_dietary is required",
                ));
            }

            let expiry = self
                .safety_disclosures
                .expiry_or_best_before
                .ok_or_else(|| {
                    AppError::validation(
                        "Food disclosures cannot conceal expiry: expiry_or_best_before timestamp is required",
                    )
                })?;
            if expiry <= now {
                return Err(AppError::validation(
                    "Expired food cannot be offered: expiry_or_best_before must be in the future",
                ));
            }
        } else if !NON_FOOD_CONDITIONS.contains(&condition) {
            return Err(AppError::validation(format!(
                "Condition must be one of {:?}",
                NON_FOOD_CONDITIONS
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSurplusInput {
    pub supplier_id: Uuid,
    pub establishment_id: Uuid,
    pub pickup_point_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub category: String,
    pub unit: String,
    pub admitted_quantity: u64,
    #[serde(default)]
    pub available_quantity: Option<u64>,
    #[serde(default)]
    pub committed_quantity: Option<u64>,
    #[serde(default)]
    pub fulfilled_quantity: Option<u64>,
    #[serde(default)]
    pub disposed_quantity: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishOfferInput {
    #[serde(default)]
    pub offer_id: Option<Uuid>,
    #[serde(default)]
    pub booking_starts_at: Option<DateTime<Utc>>,
    pub booking_ends_at: DateTime<Utc>,
    pub pickup_starts_at: DateTime<Utc>,
    pub pickup_ends_at: DateTime<Utc>,
    #[serde(default)]
    pub price_cents: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedIntervals {
    pub booking_starts_at: DateTime<Utc>,
    pub booking_ends_at: DateTime<Utc>,
    pub pickup_starts_at: DateTime<Utc>,
    pub pickup_ends_at: DateTime<Utc>,
}

impl PublishOfferInput {
    pub fn validate_intervals(&self, now: DateTime<Utc>) -> Result<ValidatedIntervals> {
        let booking_starts_at = self.booking_starts_at.unwrap_or(now);
        let booking_ends_at = self.booking_ends_at;
        let pickup_starts_at = self.pickup_starts_at;
        let pickup_ends_at = self.pickup_ends_at;

        if booking_ends_at <= booking_starts_at {
            return Err(AppError::validation(
                "Booking opening must precede booking closing (booking_ends_at must be after booking_starts_at)",
            ));
        }

        if booking_ends_at <= now {
            return Err(AppError::validation(
                "Booking deadline (booking_ends_at) must be in the future",
            ));
        }

        if pickup_ends_at <= pickup_starts_at {
            return Err(AppError::validation(
                "Pickup opening must precede pickup end (pickup_ends_at must be after pickup_starts_at)",
            ));
        }

        if pickup_ends_at <= now {
            return Err(AppError::validation(
                "Pickup end (pickup_ends_at) must be in the future",
            ));
        }

        if booking_ends_at > pickup_ends_at {
            return Err(AppError::validation(
                "Booking closing deadline cannot exceed pickup end (booking_ends_at must be <= pickup_ends_at)",
            ));
        }

        if pickup_starts_at < booking_starts_at {
            return Err(AppError::validation(
                "Pickup opening cannot precede booking opening (pickup_starts_at must be >= booking_starts_at)",
            ));
        }

        Ok(ValidatedIntervals {
            booking_starts_at,
            booking_ends_at,
            pickup_starts_at,
            pickup_ends_at,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    CreateSurplus(CreateSurplusInput),
    CreateDraft(CreateDraftInput),
    PublishOffer(PublishOfferInput),
}

pub async fn execute(
    state: &AppState,
    actor: &User,
    idempotency_key: Option<&str>,
    command: Command,
) -> Result<(StatusCode, Value)> {
    let mut tx = state.pool.begin().await?;
    let actor = identity::lock_actor(&mut tx, actor.id).await?;
    if !actor.declared_adult
        || actor.standing != "active"
        || actor.restriction == "complete_suspension"
    {
        return Err(AppError::forbidden());
    }

    let (code, result) = match &command {
        Command::CreateDraft(input) => {
            let command_json = serde_json::to_value(input).map_err(|_| AppError::internal())?;
            let (replay, cached) = Replay::begin(
                &mut tx,
                actor.id,
                "create_draft",
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

            // Validate draft input with business and safety invariants
            input.validate(Utc::now())?;

            // Lock order: supplier authority first
            let mut lock = LockOrder::default();
            lock.acquire(Authority::Supplier, &input.supplier_id.to_string())?;

            // Fetch and lock supplier
            let supplier_row: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT owner_id, standing FROM suasobra.suppliers WHERE id=$1 FOR UPDATE",
            )
            .bind(input.supplier_id)
            .fetch_optional(&mut *tx)
            .await?;

            let (owner_id, standing) = supplier_row.ok_or_else(AppError::missing)?;

            if owner_id != actor.id && !actor.moderator() {
                return Err(AppError::forbidden());
            }

            if standing == "closed" {
                return Err(AppError::validation("Supplier is closed"));
            }

            // Validate establishment belongs to supplier
            let est_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM suasobra.establishments WHERE id=$1 AND supplier_id=$2)",
            )
            .bind(input.establishment_id)
            .bind(input.supplier_id)
            .fetch_one(&mut *tx)
            .await?;

            if !est_exists {
                return Err(AppError::validation(
                    "Establishment does not exist or does not belong to this supplier",
                ));
            }

            // Validate pickup point belongs to establishment
            let point_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM suasobra.pickup_points WHERE id=$1 AND establishment_id=$2)",
            )
            .bind(input.pickup_point_id)
            .bind(input.establishment_id)
            .fetch_one(&mut *tx)
            .await?;

            if !point_exists {
                return Err(AppError::validation(
                    "Pickup point does not exist or does not belong to this establishment",
                ));
            }

            let price_cents = input.price_cents.unwrap_or(0);

            // Insert into suasobra.offers using database native uuidv7()
            let offer_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO suasobra.offers (
                    supplier_id, establishment_id, pickup_point_id,
                    title, description, category, unit, status,
                    admitted_quantity, available_quantity, committed_quantity,
                    fulfilled_quantity, disposed_quantity,
                    surplus_reason, condition, safety_disclosures,
                    price_cents, currency
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'draft', $8, $8, 0, 0, 0, $9, $10, $11, $12, 'BRL')
                RETURNING id
                "#,
            )
            .bind(input.supplier_id)
            .bind(input.establishment_id)
            .bind(input.pickup_point_id)
            .bind(input.title.trim())
            .bind(input.description.trim())
            .bind(input.category.trim())
            .bind(input.unit.trim())
            .bind(input.admitted_quantity as i64)
            .bind(input.surplus_reason.trim())
            .bind(input.condition.trim())
            .bind(serde_json::to_value(&input.safety_disclosures).unwrap_or(json!({})))
            .bind(price_cents as i64)
            .fetch_one(&mut *tx)
            .await?;

            let offer_val: Value =
                sqlx::query_scalar("SELECT to_jsonb(o) FROM suasobra.offers o WHERE id=$1")
                    .bind(offer_id)
                    .fetch_one(&mut *tx)
                    .await?;

            // Record audit fact for created draft
            let mut fact = AuditFact {
                entity_type: "offer".into(),
                entity_id: offer_id.to_string(),
                action: "draft_created".into(),
                actor_id: Some(actor.id),
                actor_scope: if actor.moderator() {
                    "moderator"
                } else {
                    "supplier_owner"
                }
                .into(),
                authority_or_policy: "2026.2-free-launch".into(),
                evidence_kind: "supplier_declaration".into(),
                reason: format!("Create surplus draft: {}", input.surplus_reason.trim()),
                prior_state: json!({}),
                resulting_state: offer_val.clone(),
                prior_quantity: Some(0),
                resulting_quantity: Some(input.admitted_quantity as i64),
                quantity_delta: Some(input.admitted_quantity as i64),
                ..Default::default()
            };
            fact.record(&mut tx, None).await?;

            replay.finish(&mut tx, 201, &offer_val).await?;
            (StatusCode::CREATED, offer_val)
        }
        Command::CreateSurplus(input) => {
            let command_json = serde_json::to_value(input).map_err(|_| AppError::internal())?;
            let (replay, cached) = Replay::begin(
                &mut tx,
                actor.id,
                "create_surplus",
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

            // Lock order: supplier authority first
            let mut lock = LockOrder::default();
            lock.acquire(Authority::Supplier, &input.supplier_id.to_string())?;

            // Fetch and lock supplier
            let supplier_row: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT owner_id, standing FROM suasobra.suppliers WHERE id=$1 FOR UPDATE",
            )
            .bind(input.supplier_id)
            .fetch_optional(&mut *tx)
            .await?;

            let (owner_id, standing) = supplier_row.ok_or_else(AppError::missing)?;

            if owner_id != actor.id && !actor.moderator() {
                return Err(AppError::forbidden());
            }

            if standing == "closed" {
                return Err(AppError::validation("Supplier is closed"));
            }

            // Validate establishment belongs to supplier
            let est_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM suasobra.establishments WHERE id=$1 AND supplier_id=$2)",
            )
            .bind(input.establishment_id)
            .bind(input.supplier_id)
            .fetch_one(&mut *tx)
            .await?;

            if !est_exists {
                return Err(AppError::validation(
                    "Establishment does not exist or does not belong to this supplier",
                ));
            }

            // Validate pickup point belongs to establishment
            let point_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM suasobra.pickup_points WHERE id=$1 AND establishment_id=$2)",
            )
            .bind(input.pickup_point_id)
            .bind(input.establishment_id)
            .fetch_one(&mut *tx)
            .await?;

            if !point_exists {
                return Err(AppError::validation(
                    "Pickup point does not exist or does not belong to this establishment",
                ));
            }

            // Validate fields
            let title = input.title.trim();
            if title.is_empty() || title.chars().count() > 255 {
                return Err(AppError::validation(
                    "Title must be between 1 and 255 characters",
                ));
            }
            let category = input.category.trim();
            if category.is_empty() || category.chars().count() > 64 {
                return Err(AppError::validation(
                    "Category must be between 1 and 64 characters",
                ));
            }
            let unit = input.unit.trim();
            if unit.is_empty() || unit.chars().count() > 64 {
                return Err(AppError::validation(
                    "Unit must be between 1 and 64 characters",
                ));
            }
            if input.admitted_quantity == 0 {
                return Err(AppError::validation("Admitted quantity must be positive"));
            }

            let available = input.available_quantity.unwrap_or(input.admitted_quantity);
            let committed = input.committed_quantity.unwrap_or(0);
            let fulfilled = input.fulfilled_quantity.unwrap_or(0);
            let disposed = input.disposed_quantity.unwrap_or(0);

            let buckets = SurplusBuckets::new(
                input.admitted_quantity,
                available,
                committed,
                fulfilled,
                disposed,
            )?;

            // Insert into suasobra.offers using database native uuidv7()
            let offer_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO suasobra.offers (
                    supplier_id, establishment_id, pickup_point_id,
                    title, description, category, unit, status,
                    admitted_quantity, available_quantity, committed_quantity,
                    fulfilled_quantity, disposed_quantity
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'draft', $8, $9, $10, $11, $12)
                RETURNING id
                "#,
            )
            .bind(input.supplier_id)
            .bind(input.establishment_id)
            .bind(input.pickup_point_id)
            .bind(title)
            .bind(input.description.trim())
            .bind(category)
            .bind(unit)
            .bind(buckets.admitted as i64)
            .bind(buckets.available as i64)
            .bind(buckets.committed as i64)
            .bind(buckets.fulfilled as i64)
            .bind(buckets.disposed as i64)
            .fetch_one(&mut *tx)
            .await?;

            let offer_val: Value =
                sqlx::query_scalar("SELECT to_jsonb(o) FROM suasobra.offers o WHERE id=$1")
                    .bind(offer_id)
                    .fetch_one(&mut *tx)
                    .await?;

            // Record audit fact for admitted surplus
            let mut fact = AuditFact {
                entity_type: "offer".into(),
                entity_id: offer_id.to_string(),
                action: "surplus_admitted".into(),
                actor_id: Some(actor.id),
                actor_scope: if actor.moderator() {
                    "moderator"
                } else {
                    "supplier_owner"
                }
                .into(),
                authority_or_policy: "2026.2-free-launch".into(),
                evidence_kind: "supplier_declaration".into(),
                reason: "Admit initial genuine surplus quantity.".into(),
                prior_state: json!({}),
                resulting_state: offer_val.clone(),
                prior_quantity: Some(0),
                resulting_quantity: Some(buckets.admitted as i64),
                quantity_delta: Some(buckets.admitted as i64),
                ..Default::default()
            };
            fact.record(&mut tx, None).await?;

            replay.finish(&mut tx, 201, &offer_val).await?;
            (StatusCode::CREATED, offer_val)
        }
        Command::PublishOffer(input) => {
            let offer_id = input
                .offer_id
                .ok_or_else(|| AppError::validation("offer_id is required to publish an offer"))?;
            let command_json = serde_json::to_value(input).map_err(|_| AppError::internal())?;
            let (replay, cached) = Replay::begin(
                &mut tx,
                actor.id,
                "publish_offer",
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

            // Fetch and lock the target offer
            let offer: Option<Offer> =
                sqlx::query_as("SELECT * FROM suasobra.offers WHERE id = $1 FOR UPDATE")
                    .bind(offer_id)
                    .fetch_optional(&mut *tx)
                    .await?;

            let offer = offer.ok_or_else(AppError::missing)?;

            // Only draft offers can be published
            if offer.status != "draft" {
                return Err(AppError::conflict(format!(
                    "Only draft offers can be published; current status is '{}'",
                    offer.status
                )));
            }

            // Lock order: supplier authority first
            let mut lock = LockOrder::default();
            lock.acquire(Authority::Supplier, &offer.supplier_id.to_string())?;

            // Fetch and lock supplier
            let supplier_row: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT owner_id, standing FROM suasobra.suppliers WHERE id=$1 FOR UPDATE",
            )
            .bind(offer.supplier_id)
            .fetch_optional(&mut *tx)
            .await?;

            let (owner_id, standing) = supplier_row.ok_or_else(AppError::missing)?;

            // Outsider check: only owner or moderator has authority (INV-018)
            if owner_id != actor.id && !actor.moderator() {
                return Err(AppError::forbidden());
            }

            // Standing check: only active or warned suppliers are eligible (INV-018)
            if standing != "active" && standing != "warned" {
                return Err(AppError::validation(format!(
                    "Supplier standing '{}' is not eligible to publish offers",
                    standing
                )));
            }

            // Validate establishment is active and belongs to supplier
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

            // Validate pickup point is active and belongs to establishment
            let point_active: Option<bool> = sqlx::query_scalar(
                "SELECT is_active FROM suasobra.pickup_points WHERE id=$1 AND establishment_id=$2 FOR UPDATE",
            )
            .bind(offer.pickup_point_id)
            .bind(offer.establishment_id)
            .fetch_optional(&mut *tx)
            .await?;

            if point_active != Some(true) {
                return Err(AppError::validation(
                    "Pickup point does not exist, does not belong to establishment, or is inactive",
                ));
            }

            // Quantity check
            if offer.admitted_quantity == 0 || offer.available_quantity == 0 {
                return Err(AppError::validation(
                    "Offer has no available quantity to publish",
                ));
            }

            // Surplus reason check (INV-016)
            if offer.surplus_reason.trim().len() < 5 {
                return Err(AppError::validation(
                    "A substantiated surplus classification is required to publish an offer",
                ));
            }

            let now = Utc::now();
            let intervals = input.validate_intervals(now)?;
            let booking_starts_at = intervals.booking_starts_at;
            let booking_ends_at = intervals.booking_ends_at;
            let pickup_starts_at = intervals.pickup_starts_at;
            let pickup_ends_at = intervals.pickup_ends_at;

            // Food safety and hard limit checks (INV-063, INV-017, INV-028)
            if offer.category == "food" {
                if !FOOD_CONDITIONS.contains(&offer.condition.as_str()) {
                    return Err(AppError::validation(format!(
                        "Food condition '{}' is invalid; unsafe or spoiled goods cannot be offered",
                        offer.condition
                    )));
                }

                let disclosures: SafetyDisclosures =
                    serde_json::from_value(offer.safety_disclosures.clone()).unwrap_or_default();
                let expiry = disclosures.expiry_or_best_before.ok_or_else(|| {
                    AppError::validation("Food disclosures must include expiry_or_best_before")
                })?;

                if expiry <= now {
                    return Err(AppError::validation(
                        "Expired food cannot be published: expiry_or_best_before must be in the future",
                    ));
                }

                if booking_ends_at > expiry {
                    return Err(AppError::validation(
                        "Booking deadline cannot exceed food expiry or best-before date",
                    ));
                }

                if pickup_ends_at > expiry {
                    return Err(AppError::validation(
                        "Pickup window end cannot exceed food expiry or best-before date",
                    ));
                }
            }

            let price_cents = input
                .price_cents
                .map(|p| p as i64)
                .unwrap_or(offer.price_cents);
            let payment_mode = if price_cents == 0 {
                "free"
            } else {
                "at_pickup"
            };

            let new_status = if booking_starts_at <= now {
                "published"
            } else {
                "scheduled"
            };

            sqlx::query(
                r#"
                UPDATE suasobra.offers
                SET status = $2,
                    booking_starts_at = $3,
                    booking_ends_at = $4,
                    pickup_starts_at = $5,
                    pickup_ends_at = $6,
                    price_cents = $7,
                    payment_mode = $8,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(offer.id)
            .bind(new_status)
            .bind(booking_starts_at)
            .bind(booking_ends_at)
            .bind(pickup_starts_at)
            .bind(pickup_ends_at)
            .bind(price_cents)
            .bind(payment_mode)
            .execute(&mut *tx)
            .await?;

            let offer_val: Value =
                sqlx::query_scalar("SELECT to_jsonb(o) FROM suasobra.offers o WHERE id=$1")
                    .bind(offer.id)
                    .fetch_one(&mut *tx)
                    .await?;

            let prior_val: Value = serde_json::to_value(&offer).unwrap_or(json!({}));

            let mut fact = AuditFact {
                entity_type: "offer".into(),
                entity_id: offer.id.to_string(),
                action: if new_status == "scheduled" {
                    "offer_scheduled".into()
                } else {
                    "offer_published".into()
                },
                actor_id: Some(actor.id),
                actor_scope: if actor.moderator() {
                    "moderator"
                } else {
                    "supplier_owner"
                }
                .into(),
                authority_or_policy: "2026.2-free-launch".into(),
                evidence_kind: "supplier_declaration".into(),
                reason: format!("Publish surplus offer: {}", offer.title),
                prior_state: prior_val,
                resulting_state: offer_val.clone(),
                prior_quantity: Some(offer.admitted_quantity),
                resulting_quantity: Some(offer.admitted_quantity),
                quantity_delta: Some(0),
                ..Default::default()
            };
            fact.record(&mut tx, None).await?;

            replay.finish(&mut tx, 200, &offer_val).await?;
            (StatusCode::OK, offer_val)
        }
    };

    tx.commit().await?;
    Ok((code, result))
}

pub async fn get_offer(pool: &PgPool, id: Uuid) -> Result<Offer> {
    sqlx::query_as::<_, Offer>("SELECT * FROM suasobra.offers WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(AppError::missing)
}

pub async fn list_offers_for_supplier(pool: &PgPool, supplier_id: Uuid) -> Result<Vec<Offer>> {
    sqlx::query_as::<_, Offer>(
        "SELECT * FROM suasobra.offers WHERE supplier_id = $1 ORDER BY created_at DESC",
    )
    .bind(supplier_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

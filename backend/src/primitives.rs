use crate::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct Money(u64);
impl TryFrom<u64> for Money {
    type Error = String;
    fn try_from(value: u64) -> std::result::Result<Self, String> {
        if value > i64::MAX as u64 {
            Err("Money overflow".into())
        } else {
            Ok(Self(value))
        }
    }
}
impl From<Money> for u64 {
    fn from(value: Money) -> Self {
        value.0
    }
}
impl Money {
    pub fn from_cents(cents: u64, currency: &str) -> Result<Self> {
        if currency != "BRL" {
            return Err(AppError::validation("Only BRL is supported."));
        }
        Self::try_from(cents).map_err(AppError::validation)
    }
    pub fn checked_add(self, other: Self) -> Result<Self> {
        Self::try_from(
            self.0
                .checked_add(other.0)
                .ok_or_else(|| AppError::validation("Money overflow"))?,
        )
        .map_err(AppError::validation)
    }
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| AppError::validation("Money cannot be negative"))
    }
    pub fn checked_mul(self, factor: u64) -> Result<Self> {
        Self::try_from(
            self.0
                .checked_mul(factor)
                .ok_or_else(|| AppError::validation("Money overflow"))?,
        )
        .map_err(AppError::validation)
    }
    pub fn cents(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct Quantity(u64);
impl TryFrom<u64> for Quantity {
    type Error = String;
    fn try_from(value: u64) -> std::result::Result<Self, String> {
        if value > i64::MAX as u64 {
            Err("Quantity overflow".into())
        } else {
            Ok(Self(value))
        }
    }
}
impl From<Quantity> for u64 {
    fn from(value: Quantity) -> Self {
        value.0
    }
}
impl Quantity {
    pub fn positive(units: u64) -> Result<Self> {
        if units == 0 {
            return Err(AppError::validation("Quantity must be positive"));
        }
        Self::try_from(units).map_err(AppError::validation)
    }
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| AppError::validation("Quantity cannot be negative"))
    }
    pub fn checked_add(self, other: Self) -> Result<Self> {
        Self::try_from(
            self.0
                .checked_add(other.0)
                .ok_or_else(|| AppError::validation("Quantity overflow"))?,
        )
        .map_err(AppError::validation)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTimeInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
impl DateTimeInterval {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self> {
        if start >= end {
            return Err(AppError::validation("Start must be before end"));
        }
        Ok(Self { start, end })
    }
    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        self.start <= at && at < self.end
    }
    pub fn expired(&self, at: DateTime<Utc>) -> bool {
        at >= self.end
    }
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        Self::new(self.start.max(other.start), self.end.min(other.end)).ok()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchPolicy {
    pub version: String,
    pub max_total_simultaneous_commitments: u32,
    pub max_simultaneous_free_reservations: u32,
    pub max_quantity_per_consumer_per_offer: u32,
    pub standard_pickup_grace_minutes: u32,
    pub free_pickup_window_minutes: u32,
    pub free_min_usable_pickup_minutes: u32,
    pub no_show_challenge_window_hours: u32,
    pub marketplace_radius_km: u32,
    pub consumer_min_age: u32,
}
impl Default for LaunchPolicy {
    fn default() -> Self {
        Self {
            version: "2026.2-free-launch".into(),
            max_total_simultaneous_commitments: 3,
            max_simultaneous_free_reservations: 2,
            max_quantity_per_consumer_per_offer: 3,
            standard_pickup_grace_minutes: 10,
            free_pickup_window_minutes: 60,
            free_min_usable_pickup_minutes: 15,
            no_show_challenge_window_hours: 24,
            marketplace_radius_km: 10,
            consumer_min_age: 18,
        }
    }
}
impl LaunchPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.version.trim().is_empty()
            || !(1..=20).contains(&self.max_total_simultaneous_commitments)
            || self.max_simultaneous_free_reservations == 0
            || self.max_simultaneous_free_reservations > self.max_total_simultaneous_commitments
            || !(1..=50).contains(&self.max_quantity_per_consumer_per_offer)
            || self.standard_pickup_grace_minutes > 60
            || !(15..=1440).contains(&self.free_pickup_window_minutes)
            || self.free_min_usable_pickup_minutes < 5
            || self.free_min_usable_pickup_minutes > self.free_pickup_window_minutes
            || !(1..=168).contains(&self.no_show_challenge_window_hours)
            || !(1..=100).contains(&self.marketplace_radius_km)
            || !(18..=120).contains(&self.consumer_min_age)
        {
            return Err(AppError::validation("Invalid launch policy"));
        }
        Ok(())
    }
}
pub struct PolicyRegistry {
    policies: BTreeMap<String, LaunchPolicy>,
    active: Option<String>,
}
impl Default for PolicyRegistry {
    fn default() -> Self {
        let policy = LaunchPolicy::default();
        Self {
            active: Some(policy.version.clone()),
            policies: [(policy.version.clone(), policy)].into(),
        }
    }
}
impl PolicyRegistry {
    pub fn register(&mut self, policy: LaunchPolicy, active: bool) -> Result<()> {
        policy.validate()?;
        if self
            .policies
            .get(&policy.version)
            .is_some_and(|existing| existing != &policy)
        {
            return Err(AppError::conflict("Policy versions are immutable"));
        }
        if active || self.active.is_none() {
            self.active = Some(policy.version.clone());
        }
        self.policies.insert(policy.version.clone(), policy);
        Ok(())
    }
    pub fn get(&self, version: &str) -> Option<&LaunchPolicy> {
        self.policies.get(version)
    }
    pub fn active(&self) -> Option<&LaunchPolicy> {
        self.active.as_ref().and_then(|v| self.get(v))
    }
}
/// Transaction-local tracking; never thread-local in a task-multiplexing runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Authority {
    Consumer,
    Allowance,
    Supplier,
    Offer,
    Reservation,
    Obligation,
}
#[derive(Default)]
pub struct LockOrder {
    last: Option<(Authority, String)>,
}
impl LockOrder {
    pub fn acquire(&mut self, level: Authority, id: &str) -> Result<()> {
        let next = (level, id.to_owned());
        if self.last.as_ref().is_some_and(|last| last > &next) {
            return Err(AppError::conflict("Lock order violated"));
        }
        self.last = Some(next);
        Ok(())
    }
}

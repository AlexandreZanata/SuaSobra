use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use suasobra_backend::{
    audit::{self, AuditFact},
    identity::{self, User},
    primitives::*,
    suppliers::PointData,
};
fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
#[test]
fn money_rejects_coercion_negative_currency_and_overflow() {
    for v in [
        json!(true),
        json!(1.5),
        json!("100"),
        json!(-1),
        json!(i64::MAX as u64 + 1),
    ] {
        assert!(serde_json::from_value::<Money>(v).is_err());
    }
    assert!(Money::from_cents(100, "USD").is_err());
    assert!(Money::from_cents(0, "BRL").is_ok());
    assert!(
        Money::from_cents(i64::MAX as u64, "BRL")
            .unwrap()
            .checked_add(Money::from_cents(1, "BRL").unwrap())
            .is_err()
    );
}
#[test]
fn money_arithmetic_is_exact() {
    let a = Money::from_cents(1050, "BRL").unwrap();
    let b = Money::from_cents(50, "BRL").unwrap();
    assert_eq!(a.checked_add(b).unwrap().cents(), 1100);
    assert_eq!(a.checked_sub(b).unwrap().cents(), 1000);
    assert_eq!(b.checked_mul(3).unwrap().cents(), 150);
    assert!(b.checked_sub(a).is_err());
}
#[test]
fn quantities_require_whole_units() {
    for v in [json!(true), json!(3.0), json!("3"), json!(-1)] {
        assert!(serde_json::from_value::<Quantity>(v).is_err());
    }
    assert!(Quantity::positive(0).is_err());
    assert!(serde_json::from_value::<Quantity>(json!(0)).is_ok());
    assert!(
        Quantity::positive(1)
            .unwrap()
            .checked_sub(Quantity::positive(2).unwrap())
            .is_err()
    );
}
#[test]
fn interval_end_is_exclusive_and_offsets_are_normalized() {
    let start = at("2026-10-09T12:00:00-04:00");
    let end = start + Duration::minutes(60);
    let interval = DateTimeInterval::new(start, end).unwrap();
    assert!(interval.contains(at("2026-10-09T16:00:00Z")));
    assert!(!interval.contains(end));
    assert!(interval.expired(end));
    assert!(DateTimeInterval::new(end, start).is_err());
    assert!(DateTimeInterval::new(start, start).is_err());
}
#[test]
fn adjacent_intervals_do_not_overlap() {
    let a = DateTimeInterval::new(at("2026-10-09T12:00:00Z"), at("2026-10-09T13:00:00Z")).unwrap();
    let b = DateTimeInterval::new(a.end, a.end + Duration::hours(1)).unwrap();
    assert!(a.intersection(&b).is_none());
    assert_eq!(a.intersection(&a).unwrap(), a);
}
#[test]
fn policy_versions_are_immutable_and_launch_has_no_billing_quota() {
    let policy = LaunchPolicy::default();
    let mut registry = PolicyRegistry::default();
    registry.register(policy.clone(), true).unwrap();
    let mut changed = policy.clone();
    changed.max_quantity_per_consumer_per_offer = 4;
    assert!(registry.register(changed, true).is_err());
    assert_eq!(registry.active(), Some(&policy));
    let v = json!(policy);
    assert!(v.get("free_plan_monthly_offer_allowance").is_none());
    assert!(v.get("online_payment_commitment_minutes").is_none());
}
#[test]
fn invalid_policy_relations_are_rejected() {
    let mut p = LaunchPolicy {
        max_simultaneous_free_reservations: 4,
        ..Default::default()
    };
    assert!(p.validate().is_err());
    p = LaunchPolicy::default();
    p.consumer_min_age = 17;
    assert!(p.validate().is_err());
}
#[test]
fn lock_tracking_is_scoped_and_sorts_peers() {
    let mut locks = LockOrder::default();
    locks.acquire(Authority::Consumer, "001").unwrap();
    locks.acquire(Authority::Supplier, "001").unwrap();
    assert!(locks.acquire(Authority::Consumer, "002").is_err());
    assert!(locks.acquire(Authority::Supplier, "000").is_err());
    let mut independent = LockOrder::default();
    independent.acquire(Authority::Consumer, "000").unwrap();
}
#[test]
fn recursive_redaction_preserves_original() {
    let original = json!({"nested":[{"password":"private","phone_number":"private","quantity":2}],"authorization":"secret"});
    let (clean, paths) = audit::redact(&original);
    assert_eq!(original["nested"][0]["password"], "private");
    assert_eq!(clean["nested"][0]["quantity"], 2);
    assert_eq!(clean["authorization"], "[REDACTED]");
    assert_eq!(
        paths,
        vec![
            "authorization",
            "nested[0].password",
            "nested[0].phone_number"
        ]
    );
}
#[test]
fn quantity_audits_require_actor_reason_and_exact_delta() {
    let mut fact = AuditFact {
        entity_type: "offer".into(),
        entity_id: "sample".into(),
        action: "adjusted".into(),
        authority_or_policy: "test".into(),
        prior_quantity: Some(2),
        resulting_quantity: Some(1),
        ..Default::default()
    };
    assert!(fact.validate().is_err());
    fact.actor_scope = "system".into();
    fact.reason = "Correction".into();
    fact.validate().unwrap();
    assert_eq!(fact.quantity_delta, Some(-1));
    fact.quantity_delta = Some(2);
    assert!(fact.validate().is_err());
}
#[test]
fn canonical_digest_is_order_independent_and_payload_sensitive() {
    assert_eq!(
        suasobra_backend::replay::digest(&json!({"a":1,"b":2})),
        suasobra_backend::replay::digest(&json!({"b":2,"a":1}))
    );
    assert_ne!(
        suasobra_backend::replay::digest(&json!({"a":1})),
        suasobra_backend::replay::digest(&json!({"a":2}))
    );
}
#[test]
fn pickup_validation_is_strict_and_rounds_exactly() {
    let mut p: PointData = serde_json::from_value(point()).unwrap();
    p.validate().unwrap();
    assert_eq!(p.approx_latitude, "-15.60");
    assert_eq!(p.latitude, "-15.601234");
    let mut v = point();
    v["standing"] = json!("active");
    assert!(serde_json::from_value::<PointData>(v).is_err());
    for v in [json!(91), json!("NaN"), Value::Null, json!(true)] {
        let mut data = point();
        data["latitude"] = v;
        let p = serde_json::from_value::<PointData>(data);
        assert!(p.is_err() || p.unwrap().validate().is_err());
    }
}
#[test]
fn invalid_time_zones_and_service_intervals_are_rejected_without_panics() {
    for time in ["24:00", "é:00", "09:99", "9:00"] {
        let mut v = point();
        v["service_intervals"][0]["start_time"] = json!(time);
        let mut p: PointData = serde_json::from_value(v).unwrap();
        assert!(p.validate().is_err());
    }
    let mut v = point();
    v["time_zone"] = json!("Invalid/Zone");
    assert!(
        serde_json::from_value::<PointData>(v)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn django_pbkdf2_password_compatibility() {
    let mut out = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(1000).unwrap(),
        b"fixture-salt",
        b"old-password",
        &mut out,
    );
    use base64::{Engine, engine::general_purpose::STANDARD};
    let encoded = format!("pbkdf2_sha256$1000$fixture-salt${}", STANDARD.encode(out));
    assert!(identity::verify_password("old-password", &encoded));
    assert!(!identity::verify_password("wrong", &encoded));
    assert!(!identity::verify_password(
        "x",
        "pbkdf2_sha256$0$salt$invalid"
    ));
}
#[test]
fn ownership_and_restrictions_do_not_erase_participant_remedies() {
    let id = "019535d9-3df7-79fb-b466-fa907fa17f9e".parse().unwrap();
    let other = "019535d9-3df7-79fb-b466-fa907fa17f9f".parse().unwrap();
    let mut user:User=serde_json::from_value(json!({"id":id,"email":"a@example.test","first_name":"","phone_number":"","is_email_verified":false,"is_phone_verified":false,"declared_adult":true,"is_active":true,"standing":"active","restriction":"none","roles":["consumer","supplier_owner"]})).unwrap();
    assert!(!user.can_reserve(id));
    assert!(user.can_reserve(other));
    user.standing = "closed".into();
    assert!(!user.can_reserve(other));
    assert!(!user.can_publish());
    assert!(user.can_access_remedy(id));
    assert!(!user.can_access_remedy(other));
}
fn point() -> Value {
    json!({"name":"Main","address_street":"Street","address_number":"1","address_neighborhood":"Center","address_city":"City","address_state":"MT","address_postal_code":"123","latitude":"-15.601234","longitude":"-56.091234","service_intervals":[{"day_of_week":1,"start_time":"09:00","end_time":"18:00"}]})
}

#[test]
fn declared_email_validation_rejects_invalid_labels_and_local_parts() {
    for invalid in [
        "a@b..test",
        "a@-example.test",
        ".a@example.test",
        "a..b@example.test",
        "a@.test",
        "a@@example.test",
        "a b@example.test",
    ] {
        assert!(identity::email(invalid).is_err(), "{invalid}");
    }
    assert_eq!(
        identity::email(" A+Tag@Example.test ").unwrap(),
        "a+tag@example.test"
    );
    assert!(PolicyRegistry::default().active().is_some());
}

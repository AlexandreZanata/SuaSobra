mod support;
use serde_json::{Value, json};
use suasobra_backend::{
    audit::{AuditFact, correction},
    identity, legacy, suppliers,
};
use support::*;
use uuid::Uuid;

#[tokio::test]
async fn migration_schema_has_native_uuidv7_and_reapplies_without_drift() {
    let db = Database::new().await;
    db.state.migrate().await.unwrap();
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert!(version >= 180006);
    let columns:Vec<(String,String)>=sqlx::query_as("SELECT table_name,column_default FROM information_schema.columns WHERE table_schema='suasobra' AND column_name='id' ORDER BY table_name").fetch_all(&db.state.pool).await.unwrap();
    assert_eq!(columns.len(), 15);
    for (table, default) in columns {
        assert_eq!(default, "uuidv7()", "{table}");
    }
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let supplier = owner.supplier(&db.state).await;
    let est = owner.establishment(&db.state, supplier).await;
    let point = owner.point(&db.state, est).await;
    for id in [owner.user.unwrap(), supplier, est, point] {
        let v: i16 = sqlx::query_scalar("SELECT uuid_extract_version($1::uuid)")
            .bind(id)
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
        assert_eq!(v, 7);
    }
    db.finish().await;
}
#[tokio::test]
async fn registration_login_rotation_logout_and_hash_storage() {
    let db = Database::new().await;
    let mut client = Client::new();
    client.bootstrap(&db.state).await;
    let anonymous = client.clone();
    let (status, user) = client
        .call(
            &db.state,
            "POST",
            "/api/v1/auth/register/",
            json!({"email":"Owner@Example.test","password":"long-password","declared_adult":true}),
        )
        .await;
    assert_eq!(status, 201, "{user}");
    assert_eq!(user["email"], "owner@example.test");
    assert_eq!(user["is_email_verified"], false);
    assert_ne!(client.cookies, anonymous.cookies);
    let password: String = sqlx::query_scalar("SELECT password FROM suasobra.users")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert!(password.starts_with("$argon2id$"));
    assert!(identity::verify_password("long-password", &password));
    let mut old = anonymous;
    assert_eq!(
        old.call(&db.state, "GET", "/api/v1/auth/session/", json!({}))
            .await
            .1["authenticated"],
        false
    );
    let prior = client.clone();
    assert_eq!(
        client
            .call(
                &db.state,
                "POST",
                "/api/v1/auth/login/",
                json!({"email":"owner@example.test","password":"long-password"})
            )
            .await
            .0,
        200
    );
    assert_ne!(client.cookies, prior.cookies);
    assert_eq!(
        client
            .call(&db.state, "POST", "/api/v1/auth/logout/", json!({}))
            .await
            .0,
        200
    );
    let mut old = prior;
    assert_eq!(
        old.call(&db.state, "GET", "/api/v1/auth/session/", json!({}))
            .await
            .1["authenticated"],
        false
    );
    assert_eq!(count(&db.state.pool, "sessions").await, 0);
    db.finish().await;
}
#[tokio::test]
async fn shared_login_limiter_rejects_sixth_failure_and_does_not_trust_forwarded_ip() {
    let db = Database::new().await;
    let mut client = Client::new();
    client.bootstrap(&db.state).await;
    for n in 0..6 {
        let (status, _) = client
            .raw(
                &db.state,
                "POST",
                "/api/v1/auth/login/",
                json!({"email":"unknown@example.test","password":"invalid"}),
                vec![(
                    "x-forwarded-for",
                    if n % 2 == 0 {
                        "203.0.113.1"
                    } else {
                        "203.0.113.2"
                    },
                )],
            )
            .await;
        assert_eq!(status, if n < 5 { 401 } else { 429 });
    }
    assert_eq!(count(&db.state.pool, "login_attempts").await, 5);
    db.finish().await;
}
#[tokio::test]
async fn supplier_lifecycle_permissions_and_persisted_events() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let id = owner.supplier(&db.state).await;
    let mut moderator = db.moderator().await;
    let mut outsider = Client::new();
    outsider.register(&db.state, "outsider").await;
    let path = format!("/api/v1/suppliers/{id}/");
    assert_eq!(
        outsider.call(&db.state, "GET", &path, json!({})).await.0,
        403
    );
    assert_eq!(
        owner
            .call(
                &db.state,
                "POST",
                &format!("{path}approve/"),
                json!({"reason":"owner bypass"})
            )
            .await
            .0,
        403
    );
    for (action, payload, standing, client) in [
        (
            "reject",
            json!({"reason":"Missing evidence","reapplication_conditions":"Provide details"}),
            "rejected",
            &mut moderator,
        ),
        (
            "reapply",
            json!({"correction_notes":"Details supplied"}),
            "pending_approval",
            &mut owner,
        ),
    ] {
        let (status, result) = client
            .call(&db.state, "POST", &format!("{path}{action}/"), payload)
            .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["standing"], standing);
    }
    for (action, standing) in [
        ("approve", "active"),
        ("warn", "warned"),
        ("clear-warning", "active"),
        ("suspend", "suspended"),
    ] {
        let (status, result) = moderator
            .call(
                &db.state,
                "POST",
                &format!("{path}{action}/"),
                json!({"reason":"Operational review"}),
            )
            .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["standing"], standing);
        assert_eq!(result["financial_eligibility"], "disabled");
    }
    assert_eq!(
        owner
            .call(
                &db.state,
                "POST",
                &format!("{path}close/"),
                json!({"reason":"Business closure"})
            )
            .await
            .0,
        200
    );
    let events = owner
        .call(&db.state, "GET", &format!("{path}events/"), json!({}))
        .await
        .1;
    assert_eq!(events.as_array().unwrap().len(), 10);
    assert_eq!(
        owner
            .call(
                &db.state,
                "POST",
                &format!("{path}close/"),
                json!({"reason":"Again"})
            )
            .await
            .0,
        409
    );
    assert_eq!(count(&db.state.pool, "supplier_events").await, 10);
    db.finish().await;
}
#[tokio::test]
async fn pickup_privacy_versions_and_owner_only_history() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let supplier = owner.supplier(&db.state).await;
    let establishment = owner.establishment(&db.state, supplier).await;
    let point = owner.point(&db.state, establishment).await;
    let path = format!("/api/v1/pickup-points/{point}/");
    let mut outsider = Client::new();
    outsider.register(&db.state, "outsider").await;
    let public = outsider.call(&db.state, "GET", &path, json!({})).await.1;
    assert!(public.get("latitude").is_none());
    assert!(public.get("longitude").is_none());
    assert_eq!(public["approx_latitude"], "-15.60");
    assert_eq!(public["address_number"], "");
    assert_eq!(
        outsider
            .call(&db.state, "GET", &format!("{path}history/"), json!({}))
            .await
            .0,
        403
    );
    assert_eq!(
        outsider
            .call(
                &db.state,
                "PATCH",
                &path,
                json!({"name":"Bypass","change_reason":"Outsider"})
            )
            .await
            .0,
        403
    );
    let (status, result) = owner
        .call(
            &db.state,
            "PATCH",
            &path,
            json!({"name":"New pickup","change_reason":"Signage changed"}),
        )
        .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["version"], 2);
    let history = owner
        .call(&db.state, "GET", &format!("{path}history/"), json!({}))
        .await
        .1;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[1]["snapshot"]["name"], "Main pickup");
    assert_eq!(history[0]["snapshot"]["name"], "New pickup");
    db.finish().await;
}
#[tokio::test]
async fn closed_supplier_rejects_point_updates_and_preserves_history() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let id = owner.supplier(&db.state).await;
    let est = owner.establishment(&db.state, id).await;
    let p = owner.point(&db.state, est).await;
    owner
        .call(
            &db.state,
            "POST",
            &format!("/api/v1/suppliers/{id}/close/"),
            json!({"reason":"Closing"}),
        )
        .await;
    assert_eq!(
        owner
            .call(
                &db.state,
                "PATCH",
                &format!("/api/v1/pickup-points/{p}/"),
                json!({"name":"Invalid","change_reason":"Late edit"})
            )
            .await
            .0,
        400
    );
    assert_eq!(count(&db.state.pool, "pickup_point_history").await, 1);
    assert_eq!(count(&db.state.pool, "pickup_points").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn command_replay_conflict_actor_scope_and_rollback() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let first = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "declare",
        None,
        Some("create-once"),
        declaration(),
    )
    .await
    .unwrap();
    let replay = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "declare",
        None,
        Some("create-once"),
        declaration(),
    )
    .await
    .unwrap();
    assert_eq!(first, replay);
    let mut changed = declaration();
    changed["trade_name"] = json!("Different");
    assert_eq!(
        suppliers::execute(
            &db.state,
            owner.user.unwrap(),
            "declare",
            None,
            Some("create-once"),
            changed
        )
        .await
        .unwrap_err()
        .status,
        409
    );
    let mut outsider = Client::new();
    outsider.register(&db.state, "other").await;
    suppliers::execute(
        &db.state,
        outsider.user.unwrap(),
        "declare",
        None,
        Some("create-once"),
        declaration(),
    )
    .await
    .unwrap();
    assert_eq!(count(&db.state.pool, "suppliers").await, 2);
    let mut invalid = declaration();
    invalid["trade_name"] = json!("");
    assert!(
        suppliers::execute(
            &db.state,
            owner.user.unwrap(),
            "declare",
            None,
            Some("failed"),
            invalid
        )
        .await
        .is_err()
    );
    assert_eq!(count(&db.state.pool, "command_replays").await, 2);
    assert_eq!(count(&db.state.pool, "audit_facts").await, 2);
    db.finish().await;
}
#[tokio::test]
async fn failed_history_insert_rolls_back_point_audit_and_replay() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let supplier = owner.supplier(&db.state).await;
    let est = owner.establishment(&db.state, supplier).await;
    let point = owner.point(&db.state, est).await;
    sqlx::raw_sql("CREATE FUNCTION suasobra.fail_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected test failure'; END $$; CREATE TRIGGER fail_history BEFORE INSERT ON suasobra.pickup_point_history FOR EACH ROW EXECUTE FUNCTION suasobra.fail_history();").execute(&db.state.pool).await.unwrap();
    let before = count(&db.state.pool, "audit_facts").await;
    let result = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "update_point",
        Some(point),
        Some("failed-update"),
        json!({"name":"Must rollback","change_reason":"Test"}),
    )
    .await;
    assert!(result.is_err());
    let version: i32 = sqlx::query_scalar("SELECT version FROM suasobra.pickup_points WHERE id=$1")
        .bind(point)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(version, 1);
    assert_eq!(count(&db.state.pool, "audit_facts").await, before);
    assert_eq!(count(&db.state.pool, "command_replays").await, 0);
    assert_eq!(count(&db.state.pool, "pickup_point_history").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn audit_outbox_corrections_redaction_and_database_immutability() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let mut tx = db.state.pool.begin().await.unwrap();
    let mut fact = AuditFact {
        entity_type: "test".into(),
        entity_id: "sample".into(),
        action: "adjusted".into(),
        actor_id: owner.user,
        reason: "Inventory correction".into(),
        authority_or_policy: "test-policy".into(),
        prior_quantity: Some(2),
        resulting_quantity: Some(1),
        payload: json!({"password":"private","safe":1}),
        idempotency_key: Some("event-one".into()),
        ..Default::default()
    };
    let id = fact.record(&mut tx, Some("test-events")).await.unwrap();
    assert_eq!(fact.record(&mut tx, Some("test-events")).await.unwrap(), id);
    tx.commit().await.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    correction(
        &mut tx,
        id,
        owner.user.unwrap(),
        "Recount",
        json!({"units":2}),
        Some(1),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(count(&db.state.pool, "audit_facts").await, 2);
    assert_eq!(count(&db.state.pool, "outbox").await, 1);
    let payload: Value = sqlx::query_scalar("SELECT payload FROM suasobra.audit_facts WHERE id=$1")
        .bind(id)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(payload["password"], "[REDACTED]");
    assert!(
        sqlx::query("UPDATE suasobra.audit_facts SET reason='tampered' WHERE id=$1")
            .bind(id)
            .execute(&db.state.pool)
            .await
            .is_err()
    );
    let mut tx = db.state.pool.begin().await.unwrap();
    fact.idempotency_key = Some("rolled-back".into());
    fact.record(&mut tx, Some("test-events")).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(count(&db.state.pool, "audit_facts").await, 2);
    assert_eq!(count(&db.state.pool, "outbox").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn tokens_are_hashed_expiring_single_use_and_transactional() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let user = owner.user.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    let token = identity::issue_token(&mut tx, user, "email_verification")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM suasobra.identity_tokens")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_ne!(stored, token);
    assert_eq!(stored, identity::hash(&token));
    let mut tx = db.state.pool.begin().await.unwrap();
    assert!(
        identity::consume_token(&mut tx, &token, "password_reset")
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    assert_eq!(
        identity::consume_token(&mut tx, &token, "email_verification")
            .await
            .unwrap(),
        user
    );
    tx.rollback().await.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    identity::consume_token(&mut tx, &token, "email_verification")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    assert!(
        identity::consume_token(&mut tx, &token, "email_verification")
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = db.state.pool.begin().await.unwrap();
    let expired = identity::issue_token(&mut tx, user, "password_reset")
        .await
        .unwrap();
    sqlx::query("UPDATE suasobra.identity_tokens SET expires_at=now() WHERE token_hash=$1")
        .bind(identity::hash(&expired))
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        identity::consume_token(&mut tx, &expired, "password_reset")
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    db.finish().await;
}
async fn legacy_fixture(db: &Database, duplicate: bool) {
    sqlx::raw_sql("CREATE TABLE public.identity_user(id bigint PRIMARY KEY,email text,password text,first_name text,phone_number text,is_email_verified boolean,is_phone_verified boolean,declared_adult boolean,is_active boolean,standing text,restriction text,roles jsonb,date_joined timestamptz); INSERT INTO public.identity_user VALUES(42,'legacy@example.test','pbkdf2_sha256$1000$salt$fixture','Legacy','',true,false,true,true,'active','none','[\"consumer\"]',now());").execute(&db.state.pool).await.unwrap();
    if duplicate {
        sqlx::query("INSERT INTO public.identity_user SELECT 43,'LEGACY@example.test',password,first_name,phone_number,is_email_verified,is_phone_verified,declared_adult,is_active,standing,restriction,roles,date_joined FROM public.identity_user WHERE id=42").execute(&db.state.pool).await.unwrap();
    }
}
#[tokio::test]
async fn legacy_import_retains_source_and_uuid_mapping_and_rejects_second_import() {
    let db = Database::new().await;
    legacy_fixture(&db, false).await;
    assert_eq!(legacy::import(&db.state.pool).await.unwrap(), 1);
    let (new_id, email): (Uuid, String) = sqlx::query_as("SELECT id,email FROM suasobra.users")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(email, "legacy@example.test");
    assert_eq!(new_id.get_version_num(), 7);
    let mapped: Uuid = sqlx::query_scalar(
        "SELECT new_id FROM suasobra.legacy_ids WHERE entity_type='user' AND legacy_id='42'",
    )
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(new_id, mapped);
    let old: i64 = sqlx::query_scalar("SELECT count(*) FROM public.identity_user WHERE id=42")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(old, 1);
    assert!(legacy::import(&db.state.pool).await.is_err());
    db.finish().await;
}
#[tokio::test]
async fn invalid_legacy_import_rolls_back_every_new_record() {
    let db = Database::new().await;
    legacy_fixture(&db, true).await;
    assert!(legacy::import(&db.state.pool).await.is_err());
    assert_eq!(count(&db.state.pool, "users").await, 0);
    assert_eq!(count(&db.state.pool, "legacy_ids").await, 0);
    assert_eq!(count(&db.state.pool, "legacy_records").await, 0);
    db.finish().await;
}

#[tokio::test]
async fn legacy_import_preserves_full_relationship_graph_and_password_login() {
    let db = Database::new().await;
    legacy_fixture(&db, false).await;
    sqlx::raw_sql(include_str!("fixtures/django-baseline.sql"))
        .execute(&db.state.pool)
        .await
        .unwrap();
    let mut derived = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(1000).unwrap(),
        b"legacy-salt",
        b"legacy-password",
        &mut derived,
    );
    use base64::{Engine, engine::general_purpose::STANDARD};
    let password = format!(
        "pbkdf2_sha256$1000$legacy-salt${}",
        STANDARD.encode(derived)
    );
    sqlx::query("UPDATE public.identity_user SET password=$1 WHERE id=42")
        .bind(password)
        .execute(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(legacy::import(&db.state.pool).await.unwrap(), 11);
    for table in [
        "users",
        "suppliers",
        "supplier_events",
        "pickup_points",
        "pickup_point_history",
        "outbox",
        "login_attempts",
    ] {
        assert_eq!(count(&db.state.pool, table).await, 1, "{table}");
    }
    assert_eq!(count(&db.state.pool, "legacy_records").await, 11);
    assert_eq!(count(&db.state.pool, "legacy_ids").await, 11);
    assert_eq!(count(&db.state.pool, "command_replays").await, 0);
    let mut client = Client::new();
    client.bootstrap(&db.state).await;
    let (status, user) = client
        .call(
            &db.state,
            "POST",
            "/api/v1/auth/login/",
            json!({"email":"legacy@example.test","password":"legacy-password"}),
        )
        .await;
    assert_eq!(status, 200, "{user}");
    let suppliers = client
        .call(&db.state, "GET", "/api/v1/suppliers/", json!({}))
        .await
        .1;
    assert_eq!(suppliers[0]["owner_id"], user["id"]);
    let supplier = suppliers[0]["id"].as_str().unwrap();
    let est = client
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/suppliers/{supplier}/establishments/"),
            json!({}),
        )
        .await
        .1;
    let establishment = est[0]["id"].as_str().unwrap();
    let points = client
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/establishments/{establishment}/pickup-points/"),
            json!({}),
        )
        .await
        .1;
    assert_eq!(points[0]["name"], "Legacy pickup");
    let point = points[0]["id"].as_str().unwrap();
    let history = client
        .call(
            &db.state,
            "GET",
            &format!("/api/v1/pickup-points/{point}/history/"),
            json!({}),
        )
        .await
        .1;
    assert_eq!(history[0]["snapshot"]["establishment_id"], 8); // Historical evidence keeps its original reference.
    let archived: Value = sqlx::query_scalar(
        "SELECT record FROM suasobra.legacy_records WHERE source_table='common_audit_fact'",
    )
    .fetch_one(&db.state.pool)
    .await
    .unwrap();
    assert_eq!(archived["entity_id"], "7");
    db.finish().await;
}

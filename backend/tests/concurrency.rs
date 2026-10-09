mod support;
use serde_json::json;
use suasobra_backend::{audit::AuditFact, identity, suppliers};
use support::*;

#[tokio::test]
async fn same_command_identity_commits_one_supplier_event_and_audit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let a = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "declare",
        None,
        Some("same-request"),
        declaration(),
    );
    let b = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "declare",
        None,
        Some("same-request"),
        declaration(),
    );
    let (a, b) = tokio::join!(a, b);
    assert_eq!(a.unwrap(), b.unwrap());
    for table in [
        "suppliers",
        "supplier_events",
        "audit_facts",
        "command_replays",
    ] {
        assert_eq!(count(&db.state.pool, table).await, 1, "{table}");
    }
    db.finish().await;
}
#[tokio::test]
async fn competing_moderator_transitions_cannot_both_commit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let id = owner.supplier(&db.state).await;
    let moderator_a = db.moderator().await;
    let moderator_b = db.moderator().await;
    let a = suppliers::execute(
        &db.state,
        moderator_a.user.unwrap(),
        "approve",
        Some(id),
        Some("approve"),
        json!({"reason":"Approved"}),
    );
    let b = suppliers::execute(
        &db.state,
        moderator_b.user.unwrap(),
        "reject",
        Some(id),
        Some("reject"),
        json!({"reason":"Rejected"}),
    );
    let (a, b) = tokio::join!(a, b);
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let error = if let Err(error) = a {
        error
    } else {
        b.unwrap_err()
    };
    assert_eq!(error.status, 409);
    let standing: String =
        sqlx::query_scalar("SELECT standing FROM suasobra.suppliers WHERE id=$1")
            .bind(id)
            .fetch_one(&db.state.pool)
            .await
            .unwrap();
    assert!(["active", "rejected"].contains(&standing.as_str()));
    assert_eq!(count(&db.state.pool, "supplier_events").await, 3);
    assert_eq!(count(&db.state.pool, "command_replays").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn competing_point_updates_keep_each_version_and_audit() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let id = owner.supplier(&db.state).await;
    let est = owner.establishment(&db.state, id).await;
    let point = owner.point(&db.state, est).await;
    let moderator = db.moderator().await;
    let a = suppliers::execute(
        &db.state,
        owner.user.unwrap(),
        "update_point",
        Some(point),
        Some("owner-edit"),
        json!({"name":"Owner edit","change_reason":"Signage"}),
    );
    let b = suppliers::execute(
        &db.state,
        moderator.user.unwrap(),
        "update_point",
        Some(point),
        Some("mod-edit"),
        json!({"name":"Operations edit","change_reason":"Correction"}),
    );
    let (a, b) = tokio::join!(a, b);
    a.unwrap();
    b.unwrap();
    let versions:Vec<i32>=sqlx::query_scalar("SELECT version FROM suasobra.pickup_point_history WHERE pickup_point_id=$1 ORDER BY version").bind(point).fetch_all(&db.state.pool).await.unwrap();
    assert_eq!(versions, vec![1, 2, 3]);
    assert_eq!(count(&db.state.pool, "audit_facts").await, 5);
    db.finish().await;
}
#[tokio::test]
async fn duplicate_audit_recognition_has_one_durable_outbox() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let task = || async {
        let mut tx = db.state.pool.begin().await.unwrap();
        let mut fact = AuditFact {
            entity_type: "test".into(),
            entity_id: "sample".into(),
            action: "recognized".into(),
            authority_or_policy: "test".into(),
            actor_id: owner.user,
            idempotency_key: Some("same-event".into()),
            ..Default::default()
        };
        let id = fact.record(&mut tx, Some("test")).await.unwrap();
        tx.commit().await.unwrap();
        id
    };
    let (a, b) = tokio::join!(task(), task());
    assert_eq!(a, b);
    assert_eq!(count(&db.state.pool, "audit_facts").await, 1);
    assert_eq!(count(&db.state.pool, "outbox").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn one_use_token_allows_only_one_parallel_consumer() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let mut tx = db.state.pool.begin().await.unwrap();
    let token = identity::issue_token(&mut tx, owner.user.unwrap(), "password_reset")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let task = || async {
        let mut tx = db.state.pool.begin().await.unwrap();
        let result = identity::consume_token(&mut tx, &token, "password_reset").await;
        if result.is_ok() {
            tx.commit().await.unwrap();
        } else {
            tx.rollback().await.unwrap();
        }
        result
    };
    let (a, b) = tokio::join!(task(), task());
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    db.finish().await;
}
#[tokio::test]
async fn simultaneous_login_failures_cannot_race_past_shared_limit() {
    let db = Database::new().await;
    let mut client = Client::new();
    client.bootstrap(&db.state).await;
    let mut tasks = vec![];
    for _ in 0..8 {
        let mut client = client.clone();
        let state = db.state.clone();
        tasks.push(tokio::spawn(async move {
            client
                .call(
                    &state,
                    "POST",
                    "/api/v1/auth/login/",
                    json!({"email":"unknown@example.test","password":"invalid"}),
                )
                .await
                .0
        }));
    }
    let mut denied = 0;
    let mut limited = 0;
    for task in tasks {
        match task.await.unwrap().as_u16() {
            401 => denied += 1,
            429 => limited += 1,
            other => panic!("Unexpected status {other}"),
        }
    }
    assert_eq!(denied, 5);
    assert_eq!(limited, 3);
    assert_eq!(count(&db.state.pool, "login_attempts").await, 5);
    db.finish().await;
}

mod support;
use serde_json::json;
use support::*;

#[tokio::test]
async fn anonymous_writes_require_csrf_and_reject_cross_origin() {
    let db = Database::new().await;
    let mut client = Client::new();
    let body = json!({"email":"a@example.test","password":"valid-password","declared_adult":true});
    assert_eq!(
        client
            .call(&db.state, "POST", "/api/v1/auth/register/", body.clone())
            .await
            .0,
        403
    );
    assert_eq!(count(&db.state.pool, "users").await, 0);
    client.bootstrap(&db.state).await;
    assert_eq!(
        client
            .raw(
                &db.state,
                "POST",
                "/api/v1/auth/register/",
                body.clone(),
                vec![("origin", "https://attacker.example")]
            )
            .await
            .0,
        403
    );
    let valid = client.csrf.clone();
    client.csrf = "a".repeat(64);
    assert_eq!(
        client
            .call(&db.state, "POST", "/api/v1/auth/register/", body.clone())
            .await
            .0,
        403
    );
    client.csrf = valid;
    assert_eq!(
        client
            .call(&db.state, "POST", "/api/v1/auth/register/", body)
            .await
            .0,
        201
    );
    db.finish().await;
}
#[tokio::test]
async fn registration_rejects_privilege_fields_and_undeclared_adult() {
    let db = Database::new().await;
    let mut client = Client::new();
    client.bootstrap(&db.state).await;
    for extra in [
        json!({"roles":["moderator"]}),
        json!({"is_email_verified":true}),
        json!({"declared_adult":false}),
        json!({"standing":"active"}),
    ] {
        let mut body =
            json!({"email":"a@example.test","password":"valid-password","declared_adult":true});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        assert_eq!(
            client
                .call(&db.state, "POST", "/api/v1/auth/register/", body)
                .await
                .0,
            400
        );
    }
    assert_eq!(count(&db.state.pool, "users").await, 0);
    db.finish().await;
}
#[tokio::test]
async fn duplicate_email_is_case_insensitive_and_race_safe_constraint() {
    let db = Database::new().await;
    let mut a = Client::new();
    let mut b = Client::new();
    a.bootstrap(&db.state).await;
    b.bootstrap(&db.state).await;
    let body = |email| json!({"email":email,"password":"valid-password","declared_adult":true});
    assert_eq!(
        a.call(
            &db.state,
            "POST",
            "/api/v1/auth/register/",
            body("a@example.test")
        )
        .await
        .0,
        201
    );
    assert_eq!(
        b.call(
            &db.state,
            "POST",
            "/api/v1/auth/register/",
            body("A@EXAMPLE.test")
        )
        .await
        .0,
        409
    );
    assert_eq!(count(&db.state.pool, "users").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn expired_and_revoked_sessions_lose_write_authority() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    sqlx::query("UPDATE suasobra.sessions SET expires_at=now() WHERE user_id=$1")
        .bind(owner.user.unwrap())
        .execute(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(
        owner
            .call(&db.state, "GET", "/api/v1/auth/session/", json!({}))
            .await
            .1["authenticated"],
        false
    );
    assert_eq!(
        owner
            .call(&db.state, "POST", "/api/v1/suppliers/", declaration())
            .await
            .0,
        403
    );
    assert_eq!(count(&db.state.pool, "suppliers").await, 0);
    db.finish().await;
}
#[tokio::test]
async fn deferred_email_and_financial_routes_cannot_create_effects() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    for action in ["verify-contact", "password-reset"] {
        for command in ["request", "confirm"] {
            let (status, value) = owner
                .call(
                    &db.state,
                    "POST",
                    &format!("/api/v1/auth/{action}/{command}/"),
                    json!({"email":"a@example.test"}),
                )
                .await;
            assert_eq!(status, 501);
            assert_eq!(value["error"]["code"], "feature_deferred");
        }
    }
    for route in ["payments", "refunds", "pix", "subscriptions"] {
        assert_eq!(
            owner
                .call(&db.state, "POST", &format!("/api/v1/{route}/"), json!({}))
                .await
                .0,
            404
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM suasobra.identity_tokens")
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(support::count(&db.state.pool, "outbox").await, 0);
    db.finish().await;
}
#[tokio::test]
async fn invalid_point_edits_leave_all_versions_unchanged() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let supplier = owner.supplier(&db.state).await;
    let est = owner.establishment(&db.state, supplier).await;
    let point = owner.point(&db.state, est).await;
    for mut payload in [
        json!({"latitude":"10"}),
        json!({"time_zone":"Invalid/Zone"}),
        json!({"version":999}),
        json!({"name":""}),
        json!({"address_street":""}),
        json!({"is_active":"true"}),
        json!({"service_intervals":[{"day_of_week":true,"start_time":"09:00","end_time":"18:00"}]}),
        json!({"latitude":91,"longitude":0}),
    ] {
        payload["change_reason"] = json!("Invalid test edit");
        let (status, value) = owner
            .call(
                &db.state,
                "PATCH",
                &format!("/api/v1/pickup-points/{point}/"),
                payload,
            )
            .await;
        assert_eq!(status, 400, "{value}");
    }
    let version: i32 = sqlx::query_scalar("SELECT version FROM suasobra.pickup_points WHERE id=$1")
        .bind(point)
        .fetch_one(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(version, 1);
    assert_eq!(count(&db.state.pool, "pickup_point_history").await, 1);
    db.finish().await;
}
#[tokio::test]
async fn unknown_resources_and_invalid_methods_use_structured_errors() {
    let db = Database::new().await;
    let mut client = Client::new();
    client.register(&db.state, "user").await;
    let (status, error) = client
        .call(&db.state, "GET", "/api/v1/not-a-route/", json!({}))
        .await;
    assert_eq!(status, 404);
    assert_eq!(error["error"]["code"], "not_found");
    let (status, error) = client
        .call(&db.state, "DELETE", "/api/v1/suppliers/", json!({}))
        .await;
    assert_eq!(status, 405);
    assert_eq!(error["error"]["code"], "method_not_allowed");
    assert_eq!(
        client
            .call(&db.state, "GET", "/api/v1/suppliers/123/", json!({}))
            .await
            .0,
        400
    );
    let schema = client
        .call(&db.state, "GET", "/api/v1/schema/", json!({}))
        .await
        .1;
    assert_eq!(schema["openapi"], "3.0.3");
    assert_eq!(
        schema["paths"]["/api/v1/suppliers/{supplier_id}/"]["get"]["parameters"][0]["schema"]["format"],
        "uuid"
    );
    db.finish().await;
}
#[tokio::test]
async fn suspended_moderator_cannot_use_stale_session_permissions() {
    let db = Database::new().await;
    let mut owner = Client::new();
    owner.register(&db.state, "owner").await;
    let supplier = owner.supplier(&db.state).await;
    let mut moderator = db.moderator().await;
    sqlx::query("UPDATE suasobra.users SET standing='suspended' WHERE id=$1")
        .bind(moderator.user.unwrap())
        .execute(&db.state.pool)
        .await
        .unwrap();
    assert_eq!(
        moderator
            .call(
                &db.state,
                "POST",
                &format!("/api/v1/suppliers/{supplier}/approve/"),
                json!({"reason":"Stale permission"})
            )
            .await
            .0,
        403
    );
    assert_eq!(count(&db.state.pool, "supplier_events").await, 1);
    db.finish().await;
}

#[tokio::test]
async fn production_cookies_are_secure_http_only_and_same_origin() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let db = Database::new().await;
    let mut state = db.state.clone();
    state.config.production = true;
    let response = suasobra_backend::http::router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf/")
                .header("host", "localhost:8000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cookies: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(
        cookies
            .iter()
            .all(|c| c.contains("; Secure") && c.contains("SameSite=Lax"))
    );
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("sessionid=") && c.contains("HttpOnly"))
    );
    let mut client = Client::new();
    client.bootstrap(&state).await;
    assert_eq!(client.raw(&state,"POST","/api/v1/auth/register/",json!({"email":"secure@example.test","password":"valid-password","declared_adult":true}),vec![("origin","http://localhost:8000")]).await.0,403);
    assert_eq!(client.raw(&state,"POST","/api/v1/auth/register/",json!({"email":"secure@example.test","password":"valid-password","declared_adult":true}),vec![("origin","https://localhost:8000")]).await.0,201);
    drop(state);
    db.finish().await;
}

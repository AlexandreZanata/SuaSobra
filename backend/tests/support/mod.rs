#![allow(dead_code)]
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU16, Ordering},
};
use suasobra_backend::{AppState, config::Config, http, identity};
use tower::ServiceExt;
use uuid::Uuid;
static IP: AtomicU16 = AtomicU16::new(1);

pub struct Database {
    pub state: AppState,
    admin: PgPool,
    name: String,
}
impl Database {
    pub async fn new() -> Self {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL is required; missing PostgreSQL is not a skipped test");
        let (prefix, parent) = url
            .rsplit_once('/')
            .expect("Test URL must have a database name");
        assert!(
            parent.ends_with("_test") && !parent.contains('?'),
            "Test administration URL must target a database ending in _test"
        );
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("Real PostgreSQL 18.6 must be available");
        let name = format!("suasobra_rust_test_{}", &identity::secret().unwrap()[..20]);
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let config = Config {
            database_url: format!("{prefix}/{name}"),
            listen: "127.0.0.1:8000".into(),
            allowed_hosts: vec!["localhost:8000".into()],
            production: false,
            max_connections: 10,
        };
        let state = AppState::connect(config)
            .await
            .expect("PostgreSQL 18.6 version check must pass");
        state.migrate().await.unwrap();
        Self { state, admin, name }
    }
    pub async fn finish(self) {
        self.state.pool.close().await;
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.name))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
    pub async fn moderator(&self) -> Client {
        let mut client = Client::new();
        client.register(&self.state, "moderator").await;
        sqlx::query("UPDATE suasobra.users SET roles='[\"consumer\",\"moderator\"]' WHERE id=$1")
            .bind(client.user.unwrap())
            .execute(&self.state.pool)
            .await
            .unwrap();
        client
    }
}
#[derive(Clone)]
pub struct Client {
    pub cookies: Vec<(String, String)>,
    pub csrf: String,
    pub user: Option<Uuid>,
    pub ip: String,
}
impl Client {
    pub fn new() -> Self {
        let next = IP.fetch_add(1, Ordering::Relaxed);
        Self {
            cookies: vec![],
            csrf: String::new(),
            user: None,
            ip: format!("127.{}.{}.1", next / 250, next % 250),
        }
    }
    pub async fn call(
        &mut self,
        state: &AppState,
        method: &str,
        path: &str,
        input: Value,
    ) -> (StatusCode, Value) {
        self.raw(state, method, path, input, vec![]).await
    }
    pub async fn raw(
        &mut self,
        state: &AppState,
        method: &str,
        path: &str,
        input: Value,
        headers: Vec<(&str, &str)>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost:8000")
            .header("content-type", "application/json")
            .header(
                "cookie",
                self.cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        if !self.csrf.is_empty() {
            req = req.header("x-csrftoken", &self.csrf);
        }
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let mut req = req
            .body(Body::from(serde_json::to_vec(&input).unwrap()))
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(
            format!("{}:12345", self.ip).parse::<SocketAddr>().unwrap(),
        ));
        let response = http::router(state.clone()).oneshot(req).await.unwrap();
        let status = response.status();
        assert!(response.headers().contains_key("x-correlation-id"));
        for header in response.headers().get_all("set-cookie") {
            let pair = header.to_str().unwrap().split(';').next().unwrap();
            let (k, v) = pair.split_once('=').unwrap();
            self.cookies.retain(|(name, _)| name != k);
            if !v.is_empty() {
                self.cookies.push((k.into(), v.into()));
            }
            if k == "csrftoken" {
                self.csrf = v.into();
            }
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        if status.is_client_error() || status.is_server_error() {
            assert!(
                value["error"]["correlation_id"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
                "{value}"
            );
        }
        (status, value)
    }
    pub async fn bootstrap(&mut self, state: &AppState) {
        let (status, value) = self
            .call(state, "GET", "/api/v1/auth/csrf/", json!({}))
            .await;
        assert_eq!(status, 200);
        assert_eq!(value["csrf_token"], self.csrf);
    }
    pub async fn register(&mut self, state: &AppState, name: &str) {
        self.bootstrap(state).await;
        let email = format!("{name}-{}@example.test", &identity::secret().unwrap()[..8]);
        let(status,value)=self.call(state,"POST","/api/v1/auth/register/",json!({"email":email,"password":"a-valid-password","declared_adult":true,"first_name":name})).await;
        assert_eq!(status, 201, "{value}");
        self.user = Some(value["id"].as_str().unwrap().parse().unwrap());
    }
    pub async fn supplier(&mut self, state: &AppState) -> Uuid {
        let (status, value) = self
            .call(state, "POST", "/api/v1/suppliers/", declaration())
            .await;
        assert_eq!(status, 201, "{value}");
        value["id"].as_str().unwrap().parse().unwrap()
    }
    pub async fn establishment(&mut self, state: &AppState, supplier: Uuid) -> Uuid {
        let (status, value) = self
            .call(
                state,
                "POST",
                &format!("/api/v1/suppliers/{supplier}/establishments/"),
                json!({"name":"Main site"}),
            )
            .await;
        assert_eq!(status, 201, "{value}");
        value["id"].as_str().unwrap().parse().unwrap()
    }
    pub async fn point(&mut self, state: &AppState, establishment: Uuid) -> Uuid {
        let (status, value) = self
            .call(
                state,
                "POST",
                &format!("/api/v1/establishments/{establishment}/pickup-points/"),
                point(),
            )
            .await;
        assert_eq!(status, 201, "{value}");
        value["id"].as_str().unwrap().parse().unwrap()
    }
}
pub fn declaration() -> Value {
    json!({"trade_name":"Surplus bakery","tax_id":"test-tax-id","business_type":"bakery","contact_email":"supplier@example.test","contact_phone":"test-phone"})
}
pub fn point() -> Value {
    json!({"name":"Main pickup","address_street":"Sample street","address_number":"10","address_neighborhood":"Center","address_city":"Sample City","address_state":"MT","address_postal_code":"78000-000","latitude":"-15.601234","longitude":"-56.091234","time_zone":"America/Cuiaba","service_intervals":[{"day_of_week":1,"start_time":"09:00","end_time":"18:00"}],"is_private_address":true})
}
pub async fn count(pool: &PgPool, table: &str) -> i64 {
    assert!(
        [
            "users",
            "sessions",
            "login_attempts",
            "suppliers",
            "supplier_events",
            "audit_facts",
            "outbox",
            "command_replays",
            "pickup_point_history",
            "pickup_points",
            "legacy_ids",
            "legacy_records"
        ]
        .contains(&table)
    );
    sqlx::query_scalar(&format!("SELECT count(*) FROM suasobra.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

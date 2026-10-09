use crate::{
    AppState,
    error::{AppError, Result},
    identity::{self, Session},
    marketplace, suppliers,
};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, State, rejection::BytesRejection},
    http::{HeaderMap, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::net::SocketAddr;
use uuid::Uuid;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/{*path}", any(handle))
        .fallback(any(handle))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}
type Reply = (u16, Value, Vec<String>);
async fn handle(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Response {
    let correlation = identity::secret().unwrap_or_else(|_| "unavailable".into());
    let ip = peer
        .map(|Extension(ConnectInfo(addr))| addr.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    let result = async {
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if !state
            .config
            .allowed_hosts
            .iter()
            .any(|allowed| allowed == host)
        {
            return Err(AppError::new(
                StatusCode::BAD_REQUEST,
                "invalid_host",
                "Invalid host.",
            ));
        }
        let bytes = body.map_err(|_| {
            AppError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "validation_error",
                "Request body is too large.",
            )
        })?;
        let safe = method == Method::GET || method == Method::HEAD || method == Method::OPTIONS;
        if !safe {
            if headers
                .get("sec-fetch-site")
                .is_some_and(|v| v == "cross-site")
            {
                return Err(AppError::forbidden());
            }
            if let Some(origin) = headers.get(header::ORIGIN) {
                let scheme = if state.config.production {
                    "https"
                } else {
                    "http"
                };
                if origin.to_str().ok() != Some(format!("{scheme}://{host}").as_str()) {
                    return Err(AppError::forbidden());
                }
            }
        }
        let path = uri.path();
        if path == "/api/v1/health/" && method == Method::GET {
            sqlx::query("SELECT 1").execute(&state.pool).await?;
            return Ok((200, json!({"status":"ok"}), vec![]));
        }
        if path == "/api/v1/schema/" && method == Method::GET {
            return Ok((
                200,
                serde_json::from_str(include_str!("../openapi.json"))
                    .map_err(|_| AppError::internal())?,
                vec![],
            ));
        }
        let session = identity::session(&state, &headers).await?;
        let input = if bytes.is_empty() {
            json!({})
        } else {
            if !headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.split(';').next() == Some("application/json"))
            {
                return Err(AppError::new(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "validation_error",
                    "Use application/json.",
                ));
            }
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| AppError::validation("Invalid JSON body"))?
        };
        let input = merge_query_and_input(uri.query(), input);
        if !safe {
            identity::csrf(session.as_ref(), &headers)?;
        }
        dispatch(
            &state,
            &method,
            path,
            &headers,
            session.as_ref(),
            input,
            &ip,
        )
        .await
    }
    .await;
    let mut response = match result {
        Ok((code, value, cookies)) => {
            let mut response = (
                StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(value),
            )
                .into_response();
            for cookie in cookies {
                if let Ok(value) = cookie.parse() {
                    response.headers_mut().append(header::SET_COOKIE, value);
                }
            }
            response
        }
        Err(error) => (
            error.status,
            Json(json!({"error":{"code":error.code,"message":error.message,
            "correlation_id":correlation,"fields":error.fields}})),
        )
            .into_response(),
    };
    if let Ok(value) = correlation.parse() {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("Static header"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("Static header"),
    );
    response
}
fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_val = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(v1), Some(v2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                result.push((v1 << 4) | v2);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(b' ');
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}
fn merge_query_and_input(query_str: Option<&str>, mut input: Value) -> Value {
    if let (Some(qs), Value::Object(map)) = (query_str, &mut input) {
        for pair in qs.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            let key = url_decode(parts.next().unwrap_or(""));
            let val = url_decode(parts.next().unwrap_or(""));
            if !key.is_empty() && !map.contains_key(&key) {
                map.insert(key, Value::String(val));
            }
        }
    }
    input
}
fn uuid(raw: &str) -> Result<Uuid> {
    raw.parse()
        .map_err(|_| AppError::validation("Expected a UUID resource identifier"))
}
async fn dispatch(
    state: &AppState,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    session: Option<&Session>,
    input: Value,
    ip: &str,
) -> Result<Reply> {
    if path == "/api/v1/auth/csrf/" && method == Method::GET {
        let mut tx = state.pool.begin().await?;
        let (token, cookies) = identity::rotate(
            &mut tx,
            session,
            session.and_then(|s| s.user_id),
            state.config.production,
        )
        .await?;
        tx.commit().await?;
        return Ok((200, json!({"csrf_token":token}), cookies));
    }
    if path == "/api/v1/auth/session/" && method == Method::GET {
        let user = match identity::user(state, session).await {
            Ok(user) => Some(user.summary()),
            Err(e) if e.status == StatusCode::UNAUTHORIZED => None,
            Err(e) => return Err(e),
        };
        return Ok((
            200,
            json!({"authenticated":user.is_some(),"user":user}),
            vec![],
        ));
    }
    if method == Method::POST {
        match path {
            "/api/v1/auth/register/" => return identity::register(state, session, input).await,
            "/api/v1/auth/login/" => return identity::login(state, session, input, ip).await,
            "/api/v1/auth/logout/" => {
                if let Some(session) = session {
                    sqlx::query("DELETE FROM suasobra.sessions WHERE id=$1")
                        .bind(session.id)
                        .execute(&state.pool)
                        .await?;
                }
                return Ok((
                    200,
                    json!({"status":"ok"}),
                    vec![
                        "sessionid=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0".into(),
                        "csrftoken=; Path=/; SameSite=Lax; Max-Age=0".into(),
                    ],
                ));
            }
            "/api/v1/auth/verify-contact/request/"
            | "/api/v1/auth/verify-contact/confirm/"
            | "/api/v1/auth/password-reset/request/"
            | "/api/v1/auth/password-reset/confirm/" => {
                return Err(AppError::new(
                    StatusCode::NOT_IMPLEMENTED,
                    "feature_deferred",
                    "Automatic email verification and recovery are deferred for launch (ADR-002).",
                ));
            }
            _ => {}
        }
    }
    let segments: Vec<_> = path.trim_matches('/').split('/').collect();
    let route = match segments.as_slice() {
        ["api", "v1", "suppliers"] => Some(("suppliers", "declare", None)),
        ["api", "v1", "suppliers", id] => Some(("supplier", "", Some(uuid(id)?))),
        ["api", "v1", "suppliers", id, "events"] => Some(("events", "", Some(uuid(id)?))),
        ["api", "v1", "suppliers", id, "establishments"] => {
            Some(("establishments", "establishment", Some(uuid(id)?)))
        }
        ["api", "v1", "suppliers", id, action]
            if [
                "approve",
                "reject",
                "reapply",
                "warn",
                "clear-warning",
                "suspend",
                "close",
            ]
            .contains(action) =>
        {
            Some(("", *action, Some(uuid(id)?)))
        }
        ["api", "v1", "establishments", id, "pickup-points"] => {
            Some(("points", "create_point", Some(uuid(id)?)))
        }
        ["api", "v1", "pickup-points", id] => Some(("point", "update_point", Some(uuid(id)?))),
        ["api", "v1", "pickup-points", id, "history"] => Some(("history", "", Some(uuid(id)?))),
        _ => None,
    };
    if let Some((query, command, id)) = route {
        let actor = identity::user(state, session).await?;
        if method == Method::GET && !query.is_empty() {
            return Ok((
                200,
                suppliers::query(state, &actor, query, id).await?,
                vec![],
            ));
        }
        let write = (method == Method::POST && command != "update_point")
            || (method == Method::PATCH && command == "update_point");
        if write && !command.is_empty() {
            let key = headers
                .get("idempotency-key")
                .map(|v| {
                    v.to_str()
                        .map_err(|_| AppError::validation("Invalid Idempotency-Key"))
                })
                .transpose()?;
            let (code, result) =
                suppliers::execute(state, actor.id, command, id, key, input).await?;
            return Ok((code, result, vec![]));
        }
        return Err(AppError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "Method not allowed.",
        ));
    }
    if (segments.as_slice() == ["api", "v1", "offers", "discover"]
        || segments.as_slice() == ["api", "v1", "discovery"])
        && (method == Method::GET || method == Method::POST)
    {
        let query: marketplace::DiscoveryQuery = serde_json::from_value(input)
            .map_err(|e| AppError::validation(format!("Invalid discovery parameters: {}", e)))?;
        let response = marketplace::discover_offers(&state.pool, query).await?;
        return Ok((
            200,
            serde_json::to_value(response).map_err(|_| AppError::internal())?,
            vec![],
        ));
    }
    if segments.len() == 4 && segments[..3] == ["api", "v1", "offers"] && method == Method::GET {
        let id = uuid(segments[3])?;
        let actor = identity::user(state, session).await.ok();
        let detail = marketplace::get_offer_detail(&state.pool, id, actor.as_ref()).await?;
        return Ok((200, detail, vec![]));
    }
    let offer_route = match segments.as_slice() {
        ["api", "v1", "offers"] => Some(("offers", "create_draft", None)),
        ["api", "v1", "offers", id] => Some(("", "", Some(uuid(id)?))),
        ["api", "v1", "offers", id, "publish"] => Some(("", "publish_offer", Some(uuid(id)?))),
        ["api", "v1", "suppliers", id, "offers"] => Some(("supplier_offers", "", Some(uuid(id)?))),
        _ => None,
    };
    if let Some((query, command, id)) = offer_route {
        let actor = identity::user(state, session).await?;
        if method == Method::GET && !query.is_empty() {
            let result = match query {
                "supplier_offers" => {
                    let offers = marketplace::list_offers_for_supplier(
                        &state.pool,
                        id.ok_or_else(AppError::missing)?,
                    )
                    .await?;
                    serde_json::to_value(offers).map_err(|_| AppError::internal())?
                }
                _ => return Err(AppError::missing()),
            };
            return Ok((200, result, vec![]));
        }
        if method == Method::POST && !command.is_empty() {
            let key = headers
                .get("idempotency-key")
                .map(|v| {
                    v.to_str()
                        .map_err(|_| AppError::validation("Invalid Idempotency-Key"))
                })
                .transpose()?;
            let (code, result) = match command {
                "create_draft" => {
                    let draft_input: marketplace::CreateDraftInput = serde_json::from_value(input)
                        .map_err(|e| {
                            AppError::validation(format!("Invalid draft payload: {}", e))
                        })?;
                    marketplace::execute(
                        state,
                        &actor,
                        key,
                        marketplace::Command::CreateDraft(draft_input),
                    )
                    .await?
                }
                "publish_offer" => {
                    let mut pub_input: marketplace::PublishOfferInput =
                        serde_json::from_value(input).map_err(|e| {
                            AppError::validation(format!("Invalid publish payload: {}", e))
                        })?;
                    if pub_input.offer_id.is_none() {
                        pub_input.offer_id = id;
                    }
                    marketplace::execute(
                        state,
                        &actor,
                        key,
                        marketplace::Command::PublishOffer(pub_input),
                    )
                    .await?
                }
                _ => return Err(AppError::missing()),
            };
            return Ok((code.as_u16(), result, vec![]));
        }
        return Err(AppError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "Method not allowed.",
        ));
    }
    if path.starts_with("/api/v1/auth/") || ["/api/v1/health/", "/api/v1/schema/"].contains(&path) {
        return Err(AppError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "Method not allowed.",
        ));
    }
    Err(AppError::missing())
}

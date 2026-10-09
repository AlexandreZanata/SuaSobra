use suasobra_backend::{AppState, config::Config, http, legacy};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("suasobra_backend=info"))
        .init();
    let config = Config::from_env().map_err(|message| format!("Configuration error: {message}"))?;
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let state = AppState::connect(config).await?;
    match command.as_str() {
        "migrate" => {
            state.migrate().await?;
            println!("Migrations applied.");
        }
        "import-django" => {
            state.migrate().await?;
            let count = legacy::import(&state.pool).await?;
            println!("Imported {count} legacy records. Existing sessions require login.");
        }
        "prune-auth" => {
            let mut tx = state.pool.begin().await?;
            sqlx::query("DELETE FROM suasobra.sessions WHERE expires_at<=now()")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM suasobra.login_attempts WHERE attempt_time<now()-interval '15 minutes'").execute(&mut *tx).await?;
            sqlx::query(
                "DELETE FROM suasobra.identity_tokens WHERE expires_at<now()-interval '7 days'",
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            println!("Expired authentication state pruned.");
        }
        "serve" => {
            let listener = tokio::net::TcpListener::bind(&state.config.listen).await?;
            tracing::info!("Backend listening");
            axum::serve(
                listener,
                http::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown())
            .await?;
        }
        _ => return Err("Usage: suasobra-backend [serve|migrate|import-django|prune-auth]".into()),
    }
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

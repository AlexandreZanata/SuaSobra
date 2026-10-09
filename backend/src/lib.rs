pub mod audit;
pub mod config;
pub mod error;
pub mod http;
pub mod identity;
pub mod legacy;
pub mod marketplace;
pub mod primitives;
pub mod replay;
pub mod suppliers;

use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: config::Config,
    pub password_slots: Arc<Semaphore>,
}

impl AppState {
    pub async fn connect(config: config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("SET search_path TO public")
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&config.database_url)
            .await?;
        let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
            .fetch_one(&pool)
            .await?;
        if version < 180006 {
            return Err("PostgreSQL 18.6 or newer is required".into());
        }
        Ok(Self {
            pool,
            config,
            password_slots: Arc::new(Semaphore::new(4)),
        })
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }
}

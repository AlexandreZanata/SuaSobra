use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub listen: String,
    pub allowed_hosts: Vec<String>,
    pub production: bool,
    pub max_connections: u32,
}
impl Config {
    pub fn from_env() -> Result<Self, String> {
        let production = match env::var("ENVIRONMENT")
            .unwrap_or_else(|_| "development".into())
            .as_str()
        {
            "production" => true,
            "development" | "test" => false,
            _ => return Err("ENVIRONMENT must be development, test or production".into()),
        };
        let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required")?;
        let allowed_hosts: Vec<String> = env::var("ALLOWED_HOSTS")
            .unwrap_or_else(|_| "localhost:8000,127.0.0.1:8000".into())
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect();
        if allowed_hosts
            .iter()
            .any(|s| s.is_empty() || s.contains('*') || s.contains('/'))
        {
            return Err("ALLOWED_HOSTS must contain explicit host[:port] values".into());
        }
        let max_connections = env::var("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".into())
            .parse::<u32>()
            .map_err(|_| "Invalid DB_MAX_CONNECTIONS")?;
        if !(1..=100).contains(&max_connections) {
            return Err("DB_MAX_CONNECTIONS must be 1..100".into());
        }
        Ok(Self {
            database_url,
            production,
            allowed_hosts,
            max_connections,
            listen: env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".into()),
        })
    }
}

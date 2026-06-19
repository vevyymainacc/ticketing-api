#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub request_timeout_secs: u64,
    pub max_in_flight: usize,
    pub pool_max_connections: u32,
    pub pool_acquire_timeout_secs: u64,
    pub statement_timeout_ms: u64,
    pub hold_ttl_secs: i64,
    pub sweep_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            request_timeout_secs: env_or("REQUEST_TIMEOUT_SECS", 10),
            max_in_flight: env_or("MAX_IN_FLIGHT", 512),
            pool_max_connections: env_or("POOL_MAX_CONNECTIONS", 20),
            pool_acquire_timeout_secs: env_or("POOL_ACQUIRE_TIMEOUT_SECS", 3),
            statement_timeout_ms: env_or("STATEMENT_TIMEOUT_MS", 5000),
            hold_ttl_secs: env_or("HOLD_TTL_SECS", 120),
            sweep_interval_secs: env_or("SWEEP_INTERVAL_SECS", 10),
        }
    }
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

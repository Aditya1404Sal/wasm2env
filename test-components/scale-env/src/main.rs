// Tests edge case #14: Large number of env vars (15).
fn main() {
    let vars = [
        std::env::var("APP_NAME").unwrap_or_default(),
        std::env::var("APP_VERSION").unwrap_or_default(),
        std::env::var("APP_ENV").unwrap_or_default(),
        std::env::var("DB_HOST").unwrap_or_default(),
        std::env::var("DB_PORT").unwrap_or_default(),
        std::env::var("DB_USER").unwrap_or_default(),
        std::env::var("DB_PASS").unwrap_or_default(),
        std::env::var("DB_NAME").unwrap_or_default(),
        std::env::var("REDIS_HOST").unwrap_or_default(),
        std::env::var("REDIS_PORT").unwrap_or_default(),
        std::env::var("SMTP_HOST").unwrap_or_default(),
        std::env::var("SMTP_PORT").unwrap_or_default(),
        std::env::var("SMTP_USER").unwrap_or_default(),
        std::env::var("LOG_LEVEL").unwrap_or_default(),
        std::env::var("SECRET_KEY").unwrap_or_default(),
    ];
    for v in &vars {
        println!("{v}");
    }
}

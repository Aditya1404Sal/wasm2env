fn main() {
    let vars = [
        std::env::var("DB_HOST").unwrap_or_default(),
        std::env::var("DB_PORT").unwrap_or_default(),
        std::env::var("DB_USER").unwrap_or_default(),
        std::env::var("DB_PASS").unwrap_or_default(),
        std::env::var("REDIS_URL").unwrap_or_default(),
        std::env::var("LOG_LEVEL").unwrap_or_default(),
        std::env::var("CORS_ORIGIN").unwrap_or_default(),
    ];
    for v in &vars {
        println!("{v}");
    }
}

fn main() {
    let db = std::env::var("DATABASE_URL").unwrap_or_default();
    let key = std::env::var("API_KEY").unwrap_or_default();
    let secret = std::env::var("JWT_SECRET").unwrap_or_default();
    println!("{db} {key} {secret}");
}

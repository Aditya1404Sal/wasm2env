fn main() {
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    println!("key: {api_key}");
}

fn main() {
    let mode = std::env::var("APP_MODE").unwrap_or_else(|_| "production".to_string());
    if mode == "debug" {
        let debug_token = std::env::var("DEBUG_TOKEN").unwrap_or_default();
        println!("debug: {debug_token}");
    } else {
        let prod_key = std::env::var("PROD_API_KEY").unwrap_or_default();
        println!("prod: {prod_key}");
    }
}

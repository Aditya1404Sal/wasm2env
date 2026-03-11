fn get_config() -> String {
    std::env::var("SERVICE_CONFIG").unwrap_or_default()
}

fn init_service() -> String {
    let cfg = get_config();
    let token = std::env::var("AUTH_TOKEN").unwrap_or_default();
    format!("{cfg}:{token}")
}

fn main() {
    let result = init_service();
    println!("{result}");
}

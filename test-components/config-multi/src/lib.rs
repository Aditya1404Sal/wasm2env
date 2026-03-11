// Expects: DATABASE_URL, API_KEY, JWT_SECRET (via wasi:config/store)
wit_bindgen::generate!({
    world: "config-multi",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        let db = wasi::config::store::get("DATABASE_URL")
            .unwrap()
            .unwrap_or_default();
        let key = wasi::config::store::get("API_KEY")
            .unwrap()
            .unwrap_or_default();
        let secret = wasi::config::store::get("JWT_SECRET")
            .unwrap()
            .unwrap_or_default();
        format!("{db} {key} {secret}")
    }
}

export!(Component);

// Expects: DB_HOST, DB_PORT, DB_USER, DB_PASS, REDIS_URL, LOG_LEVEL, CORS_ORIGIN (via wasi:config/store)
wit_bindgen::generate!({
    world: "config-many",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        let vars = [
            wasi::config::store::get("DB_HOST").unwrap().unwrap_or_default(),
            wasi::config::store::get("DB_PORT").unwrap().unwrap_or_default(),
            wasi::config::store::get("DB_USER").unwrap().unwrap_or_default(),
            wasi::config::store::get("DB_PASS").unwrap().unwrap_or_default(),
            wasi::config::store::get("REDIS_URL").unwrap().unwrap_or_default(),
            wasi::config::store::get("LOG_LEVEL").unwrap().unwrap_or_default(),
            wasi::config::store::get("CORS_ORIGIN").unwrap().unwrap_or_default(),
        ];
        vars.join(",")
    }
}

export!(Component);

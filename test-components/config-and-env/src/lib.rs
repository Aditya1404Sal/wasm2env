// Tests edge case #11: Both std::env AND wasi:config/store in the same component.
// Expects: RUNTIME_MODE (via std::env), CONFIG_DB_URL and CONFIG_API_KEY (via config/store)
wit_bindgen::generate!({
    world: "config-and-env",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        // Read one value via traditional env var
        let mode = std::env::var("RUNTIME_MODE").unwrap_or_else(|_| "default".to_string());

        // Read two values via wasi:config/store
        let db = wasi::config::store::get("CONFIG_DB_URL")
            .unwrap()
            .unwrap_or_default();
        let key = wasi::config::store::get("CONFIG_API_KEY")
            .unwrap()
            .unwrap_or_default();

        format!("mode={mode} db={db} key={key}")
    }
}

export!(Component);

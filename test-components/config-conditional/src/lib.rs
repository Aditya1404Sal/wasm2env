// Expects: APP_MODE, DEBUG_TOKEN, PROD_API_KEY (via wasi:config/store, conditional branches)
wit_bindgen::generate!({
    world: "config-conditional",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        let mode = wasi::config::store::get("APP_MODE")
            .unwrap()
            .unwrap_or_else(|| "production".to_string());
        if mode == "debug" {
            let debug_token = wasi::config::store::get("DEBUG_TOKEN")
                .unwrap()
                .unwrap_or_default();
            format!("debug: {debug_token}")
        } else {
            let prod_key = wasi::config::store::get("PROD_API_KEY")
                .unwrap()
                .unwrap_or_default();
            format!("prod: {prod_key}")
        }
    }
}

export!(Component);

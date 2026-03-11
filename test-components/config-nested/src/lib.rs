// Tests config keys accessed through nested helper functions.
// Expects: AUTH_TOKEN and SERVICE_CONFIG (via wasi:config/store)
wit_bindgen::generate!({
    world: "config-nested",
    generate_all,
});

struct Component;

fn get_config() -> String {
    wasi::config::store::get("SERVICE_CONFIG")
        .unwrap()
        .unwrap_or_default()
}

fn init_service() -> String {
    let cfg = get_config();
    let token = wasi::config::store::get("AUTH_TOKEN")
        .unwrap()
        .unwrap_or_default();
    format!("{cfg}:{token}")
}

impl Guest for Component {
    fn run() -> String {
        init_service()
    }
}

export!(Component);

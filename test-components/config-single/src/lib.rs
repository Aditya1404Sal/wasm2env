// Expects: API_KEY (via wasi:config/store)
wit_bindgen::generate!({
    world: "config-single",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        let key = wasi::config::store::get("API_KEY")
            .unwrap()
            .unwrap_or_default();
        format!("key={key}")
    }
}

export!(Component);

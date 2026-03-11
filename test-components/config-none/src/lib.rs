// Imports wasi:config/store but never calls it — should detect zero config keys
wit_bindgen::generate!({
    world: "config-none",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        let x = 2 + 2;
        format!("hello world {x}")
    }
}

export!(Component);

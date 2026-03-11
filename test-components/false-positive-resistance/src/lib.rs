// Tests edge case #12: Strings that look like env var names but are NOT
// passed to any env/config import. Should produce zero detections.
wit_bindgen::generate!({
    world: "false-positive-resistance",
    generate_all,
});

struct Component;

impl Guest for Component {
    fn run() -> String {
        // These strings look like env var names but are used as plain data,
        // never passed to std::env::var() or wasi:config/store::get().
        let error_codes = vec![
            "ERROR_CODE",
            "STATUS_OK",
            "INVALID_INPUT",
            "NOT_FOUND",
            "INTERNAL_ERROR",
        ];

        let header_names = vec![
            "CONTENT_TYPE",
            "ACCEPT_ENCODING",
            "CACHE_CONTROL",
        ];

        let mut result = String::new();
        for code in &error_codes {
            result.push_str(code);
            result.push(',');
        }
        for header in &header_names {
            result.push_str(header);
            result.push(',');
        }

        result
    }
}

export!(Component);

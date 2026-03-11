use wasm2env::{scan_wasm_bytes, scan_wasm_file};

// ===== Existing real-world components =====

#[test]
fn scan_openai_component_from_file() {
    let vars = scan_wasm_file("openai_component.wasm").unwrap();

    assert!(vars.contains(&"OPENAI_API_KEY".to_string()));
    assert!(vars.contains(&"DATABASE_URL".to_string()));
    assert!(vars.contains(&"PASSWORD_TOKEN".to_string()));
    assert_eq!(vars.len(), 3);
}

#[test]
fn scan_openai_component_from_bytes() {
    let bytes = std::fs::read("openai_component.wasm").unwrap();
    let vars = scan_wasm_bytes(&bytes).unwrap();

    assert_eq!(
        vars,
        vec!["DATABASE_URL", "OPENAI_API_KEY", "PASSWORD_TOKEN"]
    );
}

#[test]
fn scan_mcp_component() {
    let vars = scan_wasm_file("mcp_component.wasm").unwrap();

    assert!(vars.contains(&"mcp_servers".to_string()));
    assert!(vars.contains(&"meta_info".to_string()));
    assert_eq!(vars.len(), 2);
}

// ===== Test components (wasm32-wasip2, std::env::var) =====

#[test]
fn single_env_var() {
    let vars = scan_wasm_file("test-components/single-env.wasm").unwrap();
    assert_eq!(vars, vec!["API_KEY"]);
}

#[test]
fn multi_env_vars() {
    let vars = scan_wasm_file("test-components/multi-env.wasm").unwrap();
    assert_eq!(vars, vec!["API_KEY", "DATABASE_URL", "JWT_SECRET"]);
}

#[test]
fn no_env_vars() {
    let vars = scan_wasm_file("test-components/no-env.wasm").unwrap();
    assert!(
        vars.is_empty(),
        "no-env component should detect zero env vars, got: {vars:?}"
    );
}

#[test]
fn conditional_env_both_branches() {
    let vars = scan_wasm_file("test-components/conditional-env.wasm").unwrap();
    assert!(vars.contains(&"APP_MODE".to_string()));
    assert!(vars.contains(&"DEBUG_TOKEN".to_string()));
    assert!(vars.contains(&"PROD_API_KEY".to_string()));
    assert_eq!(vars.len(), 3);
}

#[test]
fn nested_function_calls() {
    let vars = scan_wasm_file("test-components/nested-calls.wasm").unwrap();
    assert_eq!(vars, vec!["AUTH_TOKEN", "SERVICE_CONFIG"]);
}

#[test]
fn many_env_vars() {
    let vars = scan_wasm_file("test-components/many-vars.wasm").unwrap();
    assert_eq!(
        vars,
        vec![
            "CORS_ORIGIN",
            "DB_HOST",
            "DB_PASS",
            "DB_PORT",
            "DB_USER",
            "LOG_LEVEL",
            "REDIS_URL",
        ]
    );
}

// ===== Test components (wasm32-wasip2, wasi:config/store) =====

#[test]
fn config_single_key() {
    let vars = scan_wasm_file("test-components/config-single.wasm").unwrap();
    assert_eq!(vars, vec!["API_KEY"]);
}

#[test]
fn config_multi_keys() {
    let vars = scan_wasm_file("test-components/config-multi.wasm").unwrap();
    assert_eq!(vars, vec!["API_KEY", "DATABASE_URL", "JWT_SECRET"]);
}

#[test]
fn config_none_no_calls() {
    let vars = scan_wasm_file("test-components/config-none.wasm").unwrap();
    assert!(
        vars.is_empty(),
        "config-none should detect zero config keys, got: {vars:?}"
    );
}

#[test]
fn config_conditional_both_branches() {
    let vars = scan_wasm_file("test-components/config-conditional.wasm").unwrap();
    assert!(vars.contains(&"APP_MODE".to_string()));
    assert!(vars.contains(&"DEBUG_TOKEN".to_string()));
    assert!(vars.contains(&"PROD_API_KEY".to_string()));
    assert_eq!(vars.len(), 3);
}

#[test]
fn config_many_keys() {
    let vars = scan_wasm_file("test-components/config-many.wasm").unwrap();
    assert_eq!(
        vars,
        vec![
            "CORS_ORIGIN",
            "DB_HOST",
            "DB_PASS",
            "DB_PORT",
            "DB_USER",
            "LOG_LEVEL",
            "REDIS_URL",
        ]
    );
}

#[test]
fn config_file_and_bytes_produce_same_result() {
    let from_file = scan_wasm_file("test-components/config-multi.wasm").unwrap();

    let bytes = std::fs::read("test-components/config-multi.wasm").unwrap();
    let from_bytes = scan_wasm_bytes(&bytes).unwrap();

    assert_eq!(from_file, from_bytes);
}

#[test]
fn config_no_rust_runtime_noise() {
    let vars = scan_wasm_file("test-components/config-many.wasm").unwrap();

    let noise = ["RUST_BACKTRACE", "CARGO_PKG_NAME", "HOME", "PATH"];
    for n in &noise {
        assert!(
            !vars.contains(&n.to_string()),
            "should not contain noise var: {n}"
        );
    }
}

// ===== General properties =====

#[test]
fn results_are_sorted() {
    let vars = scan_wasm_file("openai_component.wasm").unwrap();

    let mut sorted = vars.clone();
    sorted.sort();
    assert_eq!(vars, sorted, "scan results should be sorted alphabetically");
}

#[test]
fn no_rust_runtime_noise() {
    let vars = scan_wasm_file("test-components/many-vars.wasm").unwrap();

    let noise = ["RUST_BACKTRACE", "CARGO_PKG_NAME", "HOME", "PATH"];
    for n in &noise {
        assert!(
            !vars.contains(&n.to_string()),
            "should not contain noise var: {n}"
        );
    }
}

#[test]
fn file_and_bytes_produce_same_result() {
    let from_file = scan_wasm_file("test-components/multi-env.wasm").unwrap();

    let bytes = std::fs::read("test-components/multi-env.wasm").unwrap();
    let from_bytes = scan_wasm_bytes(&bytes).unwrap();

    assert_eq!(from_file, from_bytes);
}

#[test]
fn invalid_wasm_returns_error() {
    let garbage = vec![0x00, 0x01, 0x02, 0x03];
    assert!(scan_wasm_bytes(&garbage).is_err() || scan_wasm_bytes(&garbage).unwrap().is_empty());
}

#[test]
fn nonexistent_file_returns_error() {
    assert!(scan_wasm_file("does_not_exist.wasm").is_err());
}

// ===== Edge case tests =====

// #11: Both std::env AND wasi:config/store in the same component
#[test]
fn config_and_env_combined() {
    let vars = scan_wasm_file("test-components/config-and-env.wasm").unwrap();
    assert!(vars.contains(&"RUNTIME_MODE".to_string()));
    assert!(vars.contains(&"CONFIG_DB_URL".to_string()));
    assert!(vars.contains(&"CONFIG_API_KEY".to_string()));
    assert_eq!(vars.len(), 3);
}

// #12: Strings that look like env vars but aren't passed to env/config imports
#[test]
fn false_positive_resistance() {
    let vars = scan_wasm_file("test-components/false-positive-resistance.wasm").unwrap();
    assert!(
        vars.is_empty(),
        "false-positive-resistance should detect zero env vars, got: {vars:?}"
    );
}

// #12b: Verify specific false-positive strings are NOT detected
#[test]
fn false_positive_specific_strings() {
    let vars = scan_wasm_file("test-components/false-positive-resistance.wasm").unwrap();
    let should_not_appear = [
        "ERROR_CODE",
        "STATUS_OK",
        "INVALID_INPUT",
        "NOT_FOUND",
        "INTERNAL_ERROR",
        "CONTENT_TYPE",
        "ACCEPT_ENCODING",
        "CACHE_CONTROL",
    ];
    for s in &should_not_appear {
        assert!(
            !vars.contains(&s.to_string()),
            "should not detect non-env string: {s}"
        );
    }
}

// #13: Empty WASM component with zero embedded core modules
#[test]
fn empty_component_no_modules() {
    // Component Model binary header (magic + version + layer)
    // but with no ModuleSection payloads
    let component_header = vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic
        0x0d, 0x00, 0x01, 0x00, // Component version (layer 1)
    ];
    let result = scan_wasm_bytes(&component_header).unwrap();
    assert!(
        result.is_empty(),
        "empty component should detect zero env vars, got: {result:?}"
    );
}

// #14: Scale test with 15 env vars
#[test]
fn scale_fifteen_env_vars() {
    let vars = scan_wasm_file("test-components/scale-env.wasm").unwrap();
    assert_eq!(
        vars,
        vec![
            "APP_ENV",
            "APP_NAME",
            "APP_VERSION",
            "DB_HOST",
            "DB_NAME",
            "DB_PASS",
            "DB_PORT",
            "DB_USER",
            "LOG_LEVEL",
            "REDIS_HOST",
            "REDIS_PORT",
            "SECRET_KEY",
            "SMTP_HOST",
            "SMTP_PORT",
            "SMTP_USER",
        ]
    );
    assert_eq!(vars.len(), 15);
}

// #15: Env var names containing digits
#[test]
fn env_vars_with_digits() {
    let vars = scan_wasm_file("test-components/env-with-digits.wasm").unwrap();
    assert_eq!(
        vars,
        vec![
            "API_V2_KEY",
            "AWS_S3_BUCKET",
            "AWS_S3_REGION",
            "OAUTH2_CLIENT_ID"
        ]
    );
}

// Config keys accessed through nested helper functions
#[test]
fn config_nested_calls() {
    let vars = scan_wasm_file("test-components/config-nested.wasm").unwrap();
    assert_eq!(vars, vec!["AUTH_TOKEN", "SERVICE_CONFIG"]);
}

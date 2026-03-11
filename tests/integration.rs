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

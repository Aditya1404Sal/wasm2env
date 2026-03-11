//! `wasm2env` — Static analysis tool for detecting environment variable
//! dependencies in WASM binaries.
//!
//! Analyzes WASM Component Model and core module binaries to find which
//! environment variables they access, using call-graph-based taint analysis.
//!
//! # Architecture
//!
//! The analysis runs in two phases:
//!
//! 1. **Extraction** ([`extract`]): Parse Component Model binaries to extract
//!    embedded core WASM modules using `wasmparser`.
//!
//! 2. **Analysis** ([`analysis`]): For each core module, build a reverse call
//!    graph from WASI env/config imports, then walk every function's IR with a
//!    simulated stack ([`stack`]) to extract string arguments at call sites.
//!    Strings are validated and filtered ([`strings`]) to produce the final list.

mod analysis;
mod extract;
mod stack;
mod strings;

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use analysis::detect_env_vars;

/// Scans a WASM binary file for environment variable dependencies.
///
/// # Arguments
/// * `path` - Path to the WASM binary file
///
/// # Returns
/// * `Result<Vec<String>>` - Sorted list of detected environment variable names
///
/// # Example
/// ```no_run
/// use wasm2env::scan_wasm_file;
///
/// let env_vars = scan_wasm_file("./my-component.wasm").unwrap();
/// for var in env_vars {
///     println!("Required: {}", var);
/// }
/// ```
pub fn scan_wasm_file<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path_ref = path.as_ref();
    let data = fs::read(path_ref)
        .with_context(|| format!("Failed to read WASM file: {}", path_ref.display()))?;

    scan_wasm_bytes(&data)
}

/// Scans WASM binary bytes for environment variable dependencies.
///
/// This is the recommended interface for FFI usage (e.g., from Elixir via Rustler).
///
/// # Arguments
/// * `wasm_bytes` - Raw WASM binary data
///
/// # Returns
/// * `Result<Vec<String>>` - Sorted list of detected environment variable names
///
/// # Example
/// ```no_run
/// use wasm2env::scan_wasm_bytes;
///
/// let wasm_data = std::fs::read("./my-component.wasm").unwrap();
/// let env_vars = scan_wasm_bytes(&wasm_data).unwrap();
/// for var in env_vars {
///     println!("Required: {}", var);
/// }
/// ```
pub fn scan_wasm_bytes(wasm_bytes: &[u8]) -> Result<Vec<String>> {
    let env_vars = detect_env_vars(wasm_bytes)?;

    let mut result: Vec<String> = env_vars.into_iter().collect();
    result.sort();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_wasm_bytes_empty() {
        // Minimal valid WASM module (empty)
        let minimal_wasm = vec![
            0x00, 0x61, 0x73, 0x6d, // WASM magic number
            0x01, 0x00, 0x00, 0x00, // Version
        ];

        let result = scan_wasm_bytes(&minimal_wasm).unwrap();
        assert_eq!(result, Vec::<String>::new());
    }
}

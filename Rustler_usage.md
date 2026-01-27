## Elixir integration

```rust
// Example Rustler NIF wrapper for Elixir integration
// you'll have to place this in: native/wasm2env_nif/src/lib.rs
use rustler::{Encoder, Env, Error, Term};
use wasm2env::scan_wasm_bytes;

/// Scans WASM binary bytes and returns a list of environment variables
#[rustler::nif]
fn scan_wasm(bytes: Vec<u8>) -> Result<Vec<String>, String> {
    scan_wasm_bytes(&bytes)
        .map_err(|e| format!("WASM scan error: {}", e))
}

/// Alternative: accepts a binary reference for zero-copy
#[rustler::nif]
fn scan_wasm_binary<'a>(env: Env<'a>, binary: Term<'a>) -> Result<Vec<String>, String> {
    let bytes = binary
        .decode::<Vec<u8>>()
        .map_err(|_| "Failed to decode binary".to_string())?;
    
    scan_wasm_bytes(&bytes)
        .map_err(|e| format!("WASM scan error: {}", e))
}

rustler::init!("Elixir.Wasm2Env.Native", [scan_wasm, scan_wasm_binary]);

// Corresponding Cargo.toml for the NIF:
// 
// [package]
// name = "wasm2env_nif"
// version = "0.1.0"
// edition = "2021"
// 
// [lib]
// name = "wasm2env_nif"
// crate-type = ["cdylib"]
// 
// [dependencies]
// rustler = "0.30"
// wasm2env = { path = "../../.." }  # Adjust path to your wasm2env crate

```
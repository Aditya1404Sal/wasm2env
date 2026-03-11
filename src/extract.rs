//! Phase 1: Extract core WASM modules from Component Model binaries.
//!
//! Component Model binaries embed one or more core WASM modules inside them.
//! This module uses `wasmparser` to locate and extract those core modules so
//! they can be individually analyzed by the taint-analysis pass.

use anyhow::Result;
use wasmparser::{Parser, Payload};

/// Extract core WASM modules from a component binary.
/// If the input is already a core module, returns it as-is.
pub fn extract_core_modules(wasm_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let parser = Parser::new(0);
    let mut modules = Vec::new();
    let mut is_core_module = false;

    for payload in parser.parse_all(wasm_bytes) {
        match payload? {
            Payload::Version { encoding, .. } => {
                if encoding == wasmparser::Encoding::Module {
                    is_core_module = true;
                }
            }
            Payload::ModuleSection { range, .. } => {
                modules.push(wasm_bytes[range.start..range.end].to_vec());
            }
            _ => {}
        }
    }

    if is_core_module && modules.is_empty() {
        return Ok(vec![wasm_bytes.to_vec()]);
    }

    Ok(modules)
}

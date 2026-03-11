//! String extraction and validation for detected environment variable names.
//!
//! Responsible for:
//! - Extracting string arguments (pointer + length pairs) from the simulated stack
//! - Reading raw strings from the WASM data-segment memory map
//! - Validating that extracted strings look like real env var names
//! - Building the memory map and collecting global constants

use std::collections::{HashMap, HashSet};

use walrus::ir::Value;
use walrus::{ConstExpr, GlobalId, GlobalKind};

use crate::stack::{SVal, StackState};

// ===== Memory map & globals =====

/// Build a memory map from data segments (offset → byte).
///
/// WASM data segments define the initial memory contents. We use them
/// to read string literals that are referenced as (ptr, len) pairs.
pub fn build_memory_map(module: &walrus::Module) -> HashMap<u32, u8> {
    let mut map = HashMap::new();
    for data in module.data.iter() {
        if let walrus::DataKind::Active {
            offset: ConstExpr::Value(Value::I32(base_offset)),
            ..
        } = &data.kind
        {
            let base = *base_offset as u32;
            for (i, &byte) in data.value.iter().enumerate() {
                map.insert(base + i as u32, byte);
            }
        }
    }
    map
}

/// Collect global constant values (`GlobalId` → i32).
pub fn collect_globals(module: &walrus::Module) -> HashMap<GlobalId, i32> {
    let mut globals = HashMap::new();
    for global in module.globals.iter() {
        if let GlobalKind::Local(ConstExpr::Value(Value::I32(val))) = &global.kind {
            globals.insert(global.id(), *val);
        }
    }
    globals
}

// ===== String extraction =====

/// Extract all valid string arguments from the stack.
/// Scans for consecutive (Known(ptr), Known(len)) pairs that point to
/// valid strings in the memory map.
pub fn extract_string_args(
    state: &StackState,
    memory_map: &HashMap<u32, u8>,
    env_vars: &mut HashSet<String>,
) {
    let stack = &state.stack;
    if stack.len() < 2 {
        return;
    }

    // Scan consecutive pairs on the stack as potential (ptr, len)
    for i in 0..stack.len() - 1 {
        if let (SVal::Known(ptr), SVal::Known(len)) = (stack[i], stack[i + 1]) {
            // Interpret as unsigned — a negative i32 is a valid large u32 address
            let uptr = ptr as u32;
            let ulen = len as u32;
            if uptr > 0 && (1..=200).contains(&ulen) {
                if let Some(s) = read_string(memory_map, uptr, ulen) {
                    if is_valid_env_name(&s) {
                        env_vars.insert(s);
                    }
                }
            }
        }
    }
}

/// Read a string from the memory map at the given pointer and length.
fn read_string(memory_map: &HashMap<u32, u8>, ptr: u32, len: u32) -> Option<String> {
    if len == 0 || len > 1000 {
        return None;
    }

    // Guard against u32 overflow on ptr + len
    let end = ptr.checked_add(len)?;

    let mut bytes = Vec::with_capacity(len as usize);
    for offset in ptr..end {
        bytes.push(*memory_map.get(&offset)?);
    }

    String::from_utf8(bytes).ok()
}

// ===== Validation =====

/// Known noise strings that appear in Rust/WASM binaries but are not
/// application-level environment variables.
const ENV_BLACKLIST: &[&str] = &[
    // Rust runtime internals
    "RUST_BACKTRACE",
    "RUST_LIB_BACKTRACE",
    "RUST_MIN_STACK",
    // Common Rust/WASM noise
    "HOME",
    "PATH",
    "TERM",
    "USER",
    "LANG",
    "SHELL",
    "DISPLAY",
    // Rust compiler/tooling
    "CARGO_PKG_NAME",
    "CARGO_PKG_VERSION",
    "CARGO_MANIFEST_DIR",
    "OUT_DIR",
    "CARGO_CFG_TARGET_OS",
    "CARGO_CFG_TARGET_ARCH",
    // Unicode/ICU internals
    "General_Category",
];

/// Validate that a string is a syntactically valid environment variable name
/// and is not in the blacklist of known noise.
pub fn is_valid_env_name(s: &str) -> bool {
    let len = s.len();
    if !(2..=100).contains(&len) {
        return false;
    }

    let mut has_letter = false;
    let mut has_underscore = false;

    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' => has_letter = true,
            b'0'..=b'9' => {}
            b'_' => has_underscore = true,
            _ => return false,
        }
    }

    if !has_letter {
        return false;
    }

    // Must not start or end with underscore (Rust internal symbols)
    if s.as_bytes()[0] == b'_' || s.as_bytes()[len - 1] == b'_' {
        return false;
    }

    // Must contain an underscore or be all-uppercase 4+ chars
    if !(has_underscore
        || len >= 4
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
    {
        return false;
    }

    // Reject blacklisted noise
    !ENV_BLACKLIST.contains(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_env_name() {
        // Valid env var names
        assert!(is_valid_env_name("DATABASE_URL"));
        assert!(is_valid_env_name("API_KEY"));
        assert!(is_valid_env_name("my_var")); // lowercase with underscore
        assert!(is_valid_env_name("PORT")); // all-caps, 4+ chars
        assert!(is_valid_env_name("X11_DISPLAY"));

        // Valid with digits (#15)
        assert!(is_valid_env_name("AWS_S3_BUCKET"));
        assert!(is_valid_env_name("OAUTH2_CLIENT_ID"));
        assert!(is_valid_env_name("API_V2_KEY"));
        assert!(is_valid_env_name("AWS_S3_REGION"));
        assert!(is_valid_env_name("V2_ENDPOINT"));

        // Invalid env var names
        assert!(!is_valid_env_name("")); // empty
        assert!(!is_valid_env_name("a")); // too short
        assert!(!is_valid_env_name("S")); // too short
        assert!(!is_valid_env_name("HAS SPACE")); // space
        assert!(!is_valid_env_name("path/to/file")); // slash
        assert!(!is_valid_env_name("key=value")); // equals
        assert!(!is_valid_env_name("123")); // no letters
        assert!(!is_valid_env_name("_PRIVATE")); // starts with underscore
        assert!(!is_valid_env_name("main")); // no underscore, not all-caps
    }
}

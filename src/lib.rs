use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;
use walrus::ir::{BinaryOp, Instr, InstrSeqId, Value};
use walrus::{ConstExpr, FunctionId, GlobalId, GlobalKind, ImportKind, LocalId};
use wasmparser::{Parser, Payload};

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

// ===== Phase 1: Extract core modules from Component Model binaries =====

/// Extract core WASM modules from a component binary.
/// If the input is already a core module, returns it as-is.
fn extract_core_modules(wasm_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
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

// ===== Phase 2: Walrus-based taint analysis =====

/// Main detection function — call-graph-based, not heuristic.
fn detect_env_vars(wasm_bytes: &[u8]) -> Result<HashSet<String>> {
    let mut env_vars = HashSet::new();

    let core_modules = extract_core_modules(wasm_bytes)?;

    for module_bytes in &core_modules {
        match walrus::Module::from_buffer(module_bytes) {
            Ok(module) => {
                analyze_module(&module, &mut env_vars);
            }
            Err(_) => {}
        }
    }

    Ok(env_vars)
}

/// Find all FunctionIds that are WASI config/environment-related imports.
/// Covers:
///   - WASI preview2: `wasi:cli/environment` → `get-environment`
///   - WASI preview1: `wasi_snapshot_preview1` → `environ_get` / `environ_sizes_get`
///   - WASI config store: `wasi:config/store` → `get`
fn find_env_imports(module: &walrus::Module) -> HashSet<FunctionId> {
    let mut env_funcs = HashSet::new();
    for import in module.imports.iter() {
        if let ImportKind::Function(fid) = import.kind {
            let is_env_import =
                // WASI preview2: wasi:cli/environment@X.Y.Z get-environment
                (import.module.contains("environment") && import.name.contains("get-environment"))
                // WASI preview1: wasi_snapshot_preview1 environ_get / environ_sizes_get
                || (import.module == "wasi_snapshot_preview1" && import.name.starts_with("environ"))
                // WASI config store: wasi:config/store@X.Y.Z get
                || (import.module.contains("config/store") && import.name == "get");
            if is_env_import {
                env_funcs.insert(fid);
            }
        }
    }
    env_funcs
}

/// Build the set of all FunctionIds that transitively call any env-related import.
/// These are the "env-touching" functions — any call TO one of these functions
/// is a potential env var access point.
fn build_env_call_chain(module: &walrus::Module, env_funcs: &HashSet<FunctionId>) -> HashSet<FunctionId> {
    // Step 1: Build reverse call graph (callee → set of callers)
    let mut reverse_graph: HashMap<FunctionId, HashSet<FunctionId>> = HashMap::new();

    for func in module.funcs.iter() {
        let caller_id = func.id();
        if let walrus::FunctionKind::Local(local_func) = &func.kind {
            let callees = collect_call_targets(local_func);
            for callee in callees {
                reverse_graph
                    .entry(callee)
                    .or_default()
                    .insert(caller_id);
            }
        }
    }

    // Step 2: Unbounded BFS from all env import functions
    let mut chain = HashSet::new();
    let mut queue: VecDeque<FunctionId> = VecDeque::new();

    for &env_func in env_funcs {
        chain.insert(env_func);
        queue.push_back(env_func);
    }

    while let Some(func_id) = queue.pop_front() {
        if let Some(callers) = reverse_graph.get(&func_id) {
            for &caller in callers {
                if chain.insert(caller) {
                    queue.push_back(caller);
                }
            }
        }
    }

    chain
}

/// Collect all direct call targets from a function's IR.
fn collect_call_targets(func: &walrus::LocalFunction) -> HashSet<FunctionId> {
    let mut targets = HashSet::new();
    let entry = func.entry_block();
    collect_calls_in_seq(func, entry, &mut targets);
    targets
}

/// Recursively collect Call targets from an instruction sequence.
fn collect_calls_in_seq(
    func: &walrus::LocalFunction,
    seq_id: InstrSeqId,
    targets: &mut HashSet<FunctionId>,
) {
    let seq = func.block(seq_id);
    for (instr, _loc) in &seq.instrs {
        match instr {
            Instr::Call(c) => {
                targets.insert(c.func);
            }
            Instr::Block(b) => collect_calls_in_seq(func, b.seq, targets),
            Instr::Loop(l) => collect_calls_in_seq(func, l.seq, targets),
            Instr::IfElse(ie) => {
                collect_calls_in_seq(func, ie.consequent, targets);
                collect_calls_in_seq(func, ie.alternative, targets);
            }
            _ => {}
        }
    }
}

/// Analyze a single core WASM module for env var references
/// using call-graph-based taint analysis.
fn analyze_module(module: &walrus::Module, env_vars: &mut HashSet<String>) {
    // Find all env-related imports — if none, this module doesn't use env vars
    let env_funcs = find_env_imports(module);
    if env_funcs.is_empty() {
        return;
    }

    // Build the transitive call chain from all env imports
    let env_call_chain = build_env_call_chain(module, &env_funcs);

    let memory_map = build_memory_map(module);
    let global_values = collect_globals(module);

    for (_func_id, local_func) in module.funcs.iter_local() {
        let entry = local_func.entry_block();
        let mut state = StackState::new();
        walk_seq(
            local_func,
            entry,
            &mut state,
            &global_values,
            &memory_map,
            &env_call_chain,
            env_vars,
        );
    }
}

/// Build a memory map from data segments (offset → byte)
fn build_memory_map(module: &walrus::Module) -> HashMap<u32, u8> {
    let mut map = HashMap::new();
    for data in module.data.iter() {
        if let walrus::DataKind::Active { offset, .. } = &data.kind {
            if let ConstExpr::Value(Value::I32(base_offset)) = offset {
                let base = *base_offset as u32;
                for (i, &byte) in data.value.iter().enumerate() {
                    map.insert(base + i as u32, byte);
                }
            }
        }
    }
    map
}

/// Collect global constant values (GlobalId → i32)
fn collect_globals(module: &walrus::Module) -> HashMap<GlobalId, i32> {
    let mut globals = HashMap::new();
    for global in module.globals.iter() {
        if let GlobalKind::Local(const_expr) = &global.kind {
            if let ConstExpr::Value(Value::I32(val)) = const_expr {
                globals.insert(global.id(), *val);
            }
        }
    }
    globals
}

/// Recursively walk an instruction sequence, simulating the stack.
/// Only captures strings at call sites to functions in the env call chain.
fn walk_seq(
    func: &walrus::LocalFunction,
    seq_id: InstrSeqId,
    state: &mut StackState,
    globals: &HashMap<GlobalId, i32>,
    memory_map: &HashMap<u32, u8>,
    env_call_chain: &HashSet<FunctionId>,
    env_vars: &mut HashSet<String>,
) {
    let seq = func.block(seq_id);
    for (instr, _loc) in &seq.instrs {
        match instr {
            // Constants
            Instr::Const(c) => match c.value {
                Value::I32(v) => state.push(SVal::Known(v)),
                _ => state.push(SVal::Unknown),
            },

            // Globals
            Instr::GlobalGet(g) => {
                let val = globals
                    .get(&g.global)
                    .map(|&v| SVal::Known(v))
                    .unwrap_or(SVal::Unknown);
                state.push(val);
            }
            Instr::GlobalSet(..) => {
                state.pop();
            }

            // Locals
            Instr::LocalGet(lg) => {
                state.push(state.get_local(lg.local));
            }
            Instr::LocalSet(ls) => {
                let val = state.pop();
                state.set_local(ls.local, val);
            }
            Instr::LocalTee(lt) => {
                let val = state.peek(0);
                state.set_local(lt.local, val);
            }

            // Binary operations
            Instr::Binop(b) => {
                let rhs = state.pop();
                let lhs = state.pop();
                match b.op {
                    BinaryOp::I32Add => {
                        state.push(match (lhs, rhs) {
                            (SVal::Known(x), SVal::Known(y)) => SVal::Known(x.wrapping_add(y)),
                            _ => SVal::Unknown,
                        });
                    }
                    _ => state.push(SVal::Unknown),
                }
            }

            // Unary operations
            Instr::Unop(..) => {
                state.pop();
                state.push(SVal::Unknown);
            }

            // Memory operations
            Instr::Load(..) => {
                state.pop();
                state.push(SVal::Unknown);
            }
            Instr::Store(..) => {
                state.pop();
                state.pop();
            }

            // Function calls — the core of taint analysis
            Instr::Call(c) => {
                if env_call_chain.contains(&c.func) {
                    extract_string_args(state, memory_map, env_vars);
                }
                state.clear();
                state.push(SVal::Unknown);
            }
            Instr::CallIndirect(..) => {
                // Can't resolve indirect calls statically, but still
                // check for string args conservatively
                state.clear();
                state.push(SVal::Unknown);
            }

            // Stack manipulation
            Instr::Drop(..) => {
                state.pop();
            }
            Instr::Select(..) => {
                state.pop(); // condition
                let b = state.pop();
                let a = state.pop();
                state.push(if a == b { a } else { SVal::Unknown });
            }

            // Control flow — walk all branches
            Instr::Block(block) => {
                walk_seq(func, block.seq, state, globals, memory_map, env_call_chain, env_vars);
            }

            Instr::Loop(lp) => {
                walk_seq(func, lp.seq, state, globals, memory_map, env_call_chain, env_vars);
            }

            Instr::IfElse(ie) => {
                state.pop(); // condition

                let mut then_state = state.clone();
                let mut else_state = state.clone();

                walk_seq(func, ie.consequent, &mut then_state, globals, memory_map, env_call_chain, env_vars);
                walk_seq(func, ie.alternative, &mut else_state, globals, memory_map, env_call_chain, env_vars);

                state.clear();
                state.push(SVal::Unknown);
            }

            Instr::BrIf(..) => {
                state.pop();
            }

            Instr::Br(..) | Instr::BrTable(..) => {
                return;
            }

            Instr::Return(..) | Instr::Unreachable(..) => {
                return;
            }

            _ => {}
        }
    }
}

/// Extract all valid string arguments from the stack.
/// Scans for consecutive (Known(ptr), Known(len)) pairs that point to
/// valid strings in the memory map.
fn extract_string_args(
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
            if ptr > 0 && len >= 1 && len <= 200 {
                if let Some(s) = read_string(memory_map, ptr as u32, len as u32) {
                    if is_valid_env_name(&s) {
                        env_vars.insert(s);
                    }
                }
            }
        }
    }
}

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
fn is_valid_env_name(s: &str) -> bool {
    let len = s.len();
    if len < 2 || len > 100 {
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
    if !has_underscore && !(len >= 4 && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())) {
        return false;
    }

    // Reject blacklisted noise
    !ENV_BLACKLIST.contains(&s)
}

// ===== Helper Types =====

#[derive(Clone, Copy, Debug, PartialEq)]
enum SVal {
    Known(i32),
    Unknown,
}

#[derive(Clone)]
struct StackState {
    stack: Vec<SVal>,
    locals: HashMap<LocalId, SVal>,
}

impl StackState {
    fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
            locals: HashMap::new(),
        }
    }

    #[inline]
    fn push(&mut self, val: SVal) {
        self.stack.push(val);
    }

    #[inline]
    fn pop(&mut self) -> SVal {
        self.stack.pop().unwrap_or(SVal::Unknown)
    }

    #[inline]
    fn peek(&self, depth: usize) -> SVal {
        self.stack
            .get(self.stack.len().wrapping_sub(depth + 1))
            .copied()
            .unwrap_or(SVal::Unknown)
    }

    #[inline]
    fn clear(&mut self) {
        self.stack.clear();
    }

    #[inline]
    fn get_local(&self, id: LocalId) -> SVal {
        self.locals.get(&id).copied().unwrap_or(SVal::Unknown)
    }

    #[inline]
    fn set_local(&mut self, id: LocalId, val: SVal) {
        self.locals.insert(id, val);
    }
}

fn read_string(memory_map: &HashMap<u32, u8>, ptr: u32, len: u32) -> Option<String> {
    if len == 0 || len > 1000 {
        return None;
    }

    let mut bytes = Vec::with_capacity(len as usize);
    for offset in ptr..ptr + len {
        bytes.push(*memory_map.get(&offset)?);
    }

    String::from_utf8(bytes).ok()
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

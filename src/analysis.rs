//! Phase 2: Call-graph-based taint analysis for env var detection.
//!
//! This module orchestrates the core analysis pipeline:
//! 1. Find WASI env/config imports in each core module
//! 2. Build a transitive call chain from those imports
//! 3. Walk all functions, simulating the stack at env-related call sites
//! 4. Extract string arguments that look like env var names

use std::collections::{HashMap, HashSet, VecDeque};

use walrus::ir::{Instr, InstrSeqId};
use walrus::{FunctionId, ImportKind};

use crate::extract::extract_core_modules;
use crate::stack::{walk_seq, StackState};
use crate::strings::{build_memory_map, collect_globals};
use anyhow::Result;

/// Main detection function — call-graph-based, not heuristic.
pub fn detect_env_vars(wasm_bytes: &[u8]) -> Result<HashSet<String>> {
    let mut env_vars = HashSet::new();

    let core_modules = extract_core_modules(wasm_bytes)?;

    for module_bytes in &core_modules {
        if let Ok(module) = walrus::Module::from_buffer(module_bytes) {
            analyze_module(&module, &mut env_vars);
        }
    }

    Ok(env_vars)
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

/// Find all `FunctionIds` that are WASI config/environment-related imports.
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

/// Build the set of all `FunctionIds` that transitively call any env-related import.
/// These are the "env-touching" functions — any call TO one of these functions
/// is a potential env var access point.
fn build_env_call_chain(
    module: &walrus::Module,
    env_funcs: &HashSet<FunctionId>,
) -> HashSet<FunctionId> {
    // Step 1: Build reverse call graph (callee → set of callers)
    let mut reverse_graph: HashMap<FunctionId, HashSet<FunctionId>> = HashMap::new();

    for func in module.funcs.iter() {
        let caller_id = func.id();
        if let walrus::FunctionKind::Local(local_func) = &func.kind {
            let callees = collect_call_targets(local_func);
            for callee in callees {
                reverse_graph.entry(callee).or_default().insert(caller_id);
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

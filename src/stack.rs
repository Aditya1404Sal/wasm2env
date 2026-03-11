//! Stack simulation for WASM taint analysis.
//!
//! Provides a lightweight abstract interpretation of WASM instructions,
//! tracking integer constants through the value stack and local variables.
//! This enables extracting string arguments (pointer, length pairs) at
//! env-related call sites.

use std::collections::{HashMap, HashSet};

use walrus::ir::{BinaryOp, Instr, InstrSeqId, Value};
use walrus::{FunctionId, GlobalId, LocalId};

use crate::strings::extract_string_args;

// ===== Value types =====

/// A simplified WASM value for stack simulation.
/// We only care about tracking i32 constants (pointers / lengths).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SVal {
    Known(i32),
    Unknown,
}

// ===== Stack state =====

/// Simulated WASM value stack and locals for taint analysis.
#[derive(Clone)]
pub struct StackState {
    pub stack: Vec<SVal>,
    locals: HashMap<LocalId, SVal>,
}

impl StackState {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
            locals: HashMap::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, val: SVal) {
        self.stack.push(val);
    }

    #[inline]
    pub fn pop(&mut self) -> SVal {
        self.stack.pop().unwrap_or(SVal::Unknown)
    }

    #[inline]
    pub fn peek(&self, depth: usize) -> SVal {
        self.stack
            .get(self.stack.len().wrapping_sub(depth + 1))
            .copied()
            .unwrap_or(SVal::Unknown)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    #[inline]
    pub fn get_local(&self, id: LocalId) -> SVal {
        self.locals.get(&id).copied().unwrap_or(SVal::Unknown)
    }

    #[inline]
    pub fn set_local(&mut self, id: LocalId, val: SVal) {
        self.locals.insert(id, val);
    }
}

// ===== Instruction walker =====

/// Recursively walk an instruction sequence, simulating the stack.
/// Only captures strings at call sites to functions in the env call chain.
#[allow(clippy::too_many_lines)]
pub fn walk_seq(
    func: &walrus::LocalFunction,
    seq_id: InstrSeqId,
    state: &mut StackState,
    globals: &mut HashMap<GlobalId, i32>,
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
                    .map_or(SVal::Unknown, |&v| SVal::Known(v));
                state.push(val);
            }
            // Global set — track the mutation so subsequent reads see the new value
            Instr::GlobalSet(gs) => {
                let val = state.pop();
                if let SVal::Known(v) = val {
                    globals.insert(gs.global, v);
                } else {
                    globals.remove(&gs.global);
                }
            }
            // Single-pop instructions
            Instr::Drop(..) | Instr::BrIf(..) => {
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
                    BinaryOp::I32Sub => {
                        state.push(match (lhs, rhs) {
                            (SVal::Known(x), SVal::Known(y)) => SVal::Known(x.wrapping_sub(y)),
                            _ => SVal::Unknown,
                        });
                    }
                    _ => state.push(SVal::Unknown),
                }
            }

            // Unary operations / Memory loads — pop one, push unknown
            Instr::Unop(..) | Instr::Load(..) => {
                state.pop();
                state.push(SVal::Unknown);
            }

            // Memory stores
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
                // Can't resolve indirect calls statically
                state.clear();
                state.push(SVal::Unknown);
            }

            // Stack manipulation
            Instr::Select(..) => {
                state.pop(); // condition
                let b = state.pop();
                let a = state.pop();
                state.push(if a == b { a } else { SVal::Unknown });
            }

            // Control flow — walk all branches
            Instr::Block(block) => {
                walk_seq(
                    func,
                    block.seq,
                    state,
                    globals,
                    memory_map,
                    env_call_chain,
                    env_vars,
                );
            }

            Instr::Loop(lp) => {
                walk_seq(
                    func,
                    lp.seq,
                    state,
                    globals,
                    memory_map,
                    env_call_chain,
                    env_vars,
                );
            }

            Instr::IfElse(ie) => {
                state.pop(); // condition

                let mut then_state = state.clone();
                let mut else_state = state.clone();

                walk_seq(
                    func,
                    ie.consequent,
                    &mut then_state,
                    globals,
                    memory_map,
                    env_call_chain,
                    env_vars,
                );
                walk_seq(
                    func,
                    ie.alternative,
                    &mut else_state,
                    globals,
                    memory_map,
                    env_call_chain,
                    env_vars,
                );

                state.clear();
                state.push(SVal::Unknown);
            }

            Instr::Br(..) | Instr::BrTable(..) | Instr::Return(..) | Instr::Unreachable(..) => {
                return;
            }

            _ => {}
        }
    }
}

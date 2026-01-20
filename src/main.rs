use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use wasmparser::{ConstExpr, Operator, Parser, Payload};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: direct-string-scanner <wasm-file>");
        eprintln!();
        eprintln!("Detects environment variables by scanning:");
        eprintln!("  1. Direct string literals in function code");
        eprintln!("  2. Error messages mentioning env vars");
        return Ok(());
    }

    let path = &args[1];
    let data = fs::read(path).with_context(|| format!("Failed to read file: {}", path))?;

    println!("🔍 Scanning for environment variable dependencies...");
    println!("File: {}", path);
    println!("---------------------------------------------------\n");

    let env_vars = scan_for_env_vars(&data)?;

    if env_vars.is_empty() {
        println!("⚠️  No environment variable dependencies detected.");
    } else {
        println!("📋 Required Environment Variables ({}):\n", env_vars.len());

        let mut sorted: Vec<_> = env_vars.into_iter().collect();
        sorted.sort();

        for (i, var_name) in sorted.iter().enumerate() {
            println!("  {}. {}", i + 1, var_name);
        }

        println!("\n💡 Users must configure these in wasmcloud before deployment.");
    }

    println!("\n---------------------------------------------------");
    Ok(())
}

#[derive(Debug, Clone)]
struct DataSegment {
    offset: u32,
    data: Vec<u8>,
}

fn scan_for_env_vars(data: &[u8]) -> Result<HashSet<String>> {
    let parser = Parser::new(0);

    let mut data_segments: Vec<DataSegment> = Vec::new();
    let mut global_values: Vec<i32> = Vec::new();
    let mut env_vars = HashSet::new();

    // First pass: collect data and globals
    for payload in parser.parse_all(data) {
        match payload? {
            Payload::DataSection(reader) => {
                for data_entry in reader {
                    let data_entry = data_entry?;

                    if let wasmparser::DataKind::Active {
                        memory_index: _,
                        offset_expr,
                    } = data_entry.kind
                    {
                        if let Some(offset) = extract_const_offset(&offset_expr) {
                            data_segments.push(DataSegment {
                                offset,
                                data: data_entry.data.to_vec(),
                            });
                        }
                    }
                }
            }

            Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global?;
                    if let Some(value) = extract_const_i32(&global.init_expr) {
                        global_values.push(value);
                    }
                }
            }

            _ => {}
        }
    }

    let memory_map = build_memory_map(&data_segments);

    // Second pass: analyze functions
    let parser = Parser::new(0);
    for payload in parser.parse_all(data) {
        if let Ok(Payload::CodeSectionEntry(body)) = payload {
            scan_function_deterministic(body, &global_values, &memory_map, &mut env_vars)?;
        }
    }

    Ok(env_vars)
}

use wasmparser::FunctionBody;

#[derive(Clone, Copy, Debug, PartialEq)]
enum AbstractValue {
    /// A known 32-bit integer constant (used for pointers and lengths)
    Known(i32),
    /// A value we can't determine statically (results of function calls, etc.)
    Unknown,
}

struct StackFrame {
    // The Wasm Stack
    stack: Vec<AbstractValue>,
    // The Wasm Locals (Index -> Value)
    locals: HashMap<u32, AbstractValue>,
}

impl StackFrame {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            locals: HashMap::new(),
        }
    }

    fn push(&mut self, val: AbstractValue) {
        self.stack.push(val);
    }

    fn pop(&mut self) -> AbstractValue {
        self.stack.pop().unwrap_or(AbstractValue::Unknown)
    }

    fn peek(&self, depth: usize) -> AbstractValue {
        if depth < self.stack.len() {
            self.stack[self.stack.len() - 1 - depth]
        } else {
            AbstractValue::Unknown
        }
    }
}

fn scan_function_deterministic(
    body: FunctionBody,
    global_values: &[i32],
    memory_map: &HashMap<u32, u8>,
    env_vars: &mut HashSet<String>,
) -> Result<()> {
    let mut reader = body.get_binary_reader();
    let mut frame = StackFrame::new();

    // 1. Initialize Locals
    // Wasm function arguments are the first N locals. We don't know them, so they are Unknown.
    // We strictly track the locals defined in the function body as "Unknown" initially.
    let local_count = reader.read_var_u32()?;
    for _ in 0..local_count {
        reader.read_var_u32()?;
        reader.read::<wasmparser::ValType>()?;
    }

    // 2. Simulate Instructions
    while !reader.eof() {
        let op = reader.read_operator()?;

        match op {
            // Add these missing operators for 100% coverage:
            Operator::I32Eqz => {
                frame.pop();
                frame.push(AbstractValue::Unknown); // Boolean result
            }

            Operator::I32Eq | Operator::I32Ne | Operator::I32LtS | Operator::I32LtU => {
                frame.pop();
                frame.pop();
                frame.push(AbstractValue::Unknown); // Comparison result
            }

            Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
                // For simple cases, we can ignore control flow
                // Strings are usually loaded before branches
            }

            Operator::Br { .. } | Operator::BrIf { .. } => {
                // Branching - conservative: keep stack as-is
            }

            Operator::Return => {
                // Function returns - we're done with this function
                break;
            }
            // --- CONSTANTS ---
            Operator::I32Const { value } => {
                frame.push(AbstractValue::Known(value));
            }
            Operator::I64Const { .. } | Operator::F32Const { .. } | Operator::F64Const { .. } => {
                frame.push(AbstractValue::Unknown); // We only care about i32 (pointers/lengths)
            }

            // --- GLOBALS ---
            Operator::GlobalGet { global_index } => {
                if let Some(&val) = global_values.get(global_index as usize) {
                    frame.push(AbstractValue::Known(val));
                } else {
                    frame.push(AbstractValue::Unknown);
                }
            }

            // --- LOCALS (The key to fixing your issue) ---
            Operator::LocalGet { local_index } => {
                let val = frame
                    .locals
                    .get(&local_index)
                    .cloned()
                    .unwrap_or(AbstractValue::Unknown);
                frame.push(val);
            }
            Operator::LocalSet { local_index } => {
                let val = frame.pop();
                frame.locals.insert(local_index, val);
            }
            Operator::LocalTee { local_index } => {
                let val = frame.peek(0); // Copy top of stack
                frame.locals.insert(local_index, val);
            }

            // --- ARITHMETIC (Crucial for Pointer + Offset calculations) ---
            Operator::I32Add => {
                let b = frame.pop();
                let a = frame.pop();
                match (a, b) {
                    (AbstractValue::Known(v1), AbstractValue::Known(v2)) => {
                        // Wrapping add to mimic Wasm behavior
                        frame.push(AbstractValue::Known(v1.wrapping_add(v2)));
                    }
                    _ => frame.push(AbstractValue::Unknown),
                }
            }
            // Add other math ops (sub, mul) here if you want higher precision,
            // but `Add` is the most important for memory offsets.
            Operator::I32Sub | Operator::I32Mul | Operator::I32DivS | Operator::I32DivU => {
                frame.pop();
                frame.pop();
                frame.push(AbstractValue::Unknown);
            }

            // --- CALLS (The Detection Logic) ---
            Operator::Call { .. } | Operator::CallIndirect { .. } => {
                // We check the top 2 items on the stack for (ptr, len) pattern.
                // Standard string passing in Rust/Wasm is often (ptr, len).

                let arg_len = frame.peek(0);
                let arg_ptr = frame.peek(1);

                // Check if they are valid pointer/length constants
                if let (AbstractValue::Known(ptr), AbstractValue::Known(len)) = (arg_ptr, arg_len) {
                    // Sanity checks: reasonable length, non-null pointer
                    if len > 0 && len < 200 && ptr > 0 {
                        if let Some(s) =
                            extract_string_from_memory(memory_map, ptr as u32, len as u32)
                        {
                            if is_env_var_candidate(&s) {
                                env_vars.insert(s);
                            }
                        }
                    }
                }

                // NOTE: Strictly speaking, we should pop the function's arguments
                // and push its return value. Since we don't have the Type signature
                // handy here easily, we can conservatively assume the stack is "dirty"
                // or just push Unknown.
                // For a robust parser, you'd look up the function type index to know
                // how many items to pop/push.
                // For *this* specific use case, we can usually ignore the stack correction
                // because we only care about the moment *before* the call.
                frame.push(AbstractValue::Unknown);
            }

            // --- DROP / SELECT ---
            Operator::Drop => {
                frame.pop();
            }
            Operator::Select => {
                frame.pop(); // condition
                let v2 = frame.pop();
                let v1 = frame.pop();
                // If both paths result in the same constant, we know it!
                if v1 == v2 {
                    frame.push(v1);
                } else {
                    frame.push(AbstractValue::Unknown);
                }
            }

            // Catch-all for other opcodes (Block, Loop, Br, etc.)
            // Handling control flow (Branches) perfectly requires a Graph parser.
            // For finding static strings, linear execution is usually sufficient
            // because string constants are rarely conditionally loaded.
            _ => {}
        }
    }

    Ok(())
}

fn is_env_var_candidate(s: &str) -> bool {
    // 1. Basic Sanity
    if s.len() < 3 || s.len() > 100 {
        return false;
    }

    // 2. Must have at least one letter
    if !s.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    // 3. Only alphanumeric and underscore
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }

    // 4. Exclude Rust Internals & Mangling
    if s.starts_with("_ZN") || s.starts_with("__") || s.contains("17h") {
        return false;
    }
    if s.ends_with("_") || s.starts_with("_") {
        return false;
    }

    let upper = s.to_uppercase();

    // 5. Exclude Noise (Updated with your list + a few critical ones)
    let noise = [
        "HTTP",
        "HTTPS",
        "JSON",
        "UTF8",
        "WASM",
        "COMPONENT",
        "RUST_BACKTRACE",
        "RUST_LOG",
        "RUST_LIB_BACKTRACE",
        "LOCALHOST",
        "MAIN",
        "STD",
        "CORE",
        "ALLOC",
    ];
    // Check against noise (using exact match is safer than contains for short words)
    if noise.contains(&upper.as_str()) {
        return false;
    }
    // Keep 'contains' only for the long distinctive rust vars
    if upper.contains("RUST_BACKTRACE") || upper.contains("RUST_LIB_BACKTRA") {
        return false;
    }

    // 6. CATEGORY A: SCREAMING_SNAKE_CASE
    // Has underscore + mostly uppercase.
    // e.g. "DATABASE_URL"
    let is_screaming_snake = s.contains('_')
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        && s.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3;

    if is_screaming_snake {
        return true;
    }

    // 7. CATEGORY B: Heuristic Keywords (The "Loose" Check)
    // We look for common patterns, but we are careful.

    // Substring checks (Must have underscore to avoid "MONKEY")
    let has_keyword_suffix = upper.contains("_KEY")
        || upper.contains("_TOKEN")
        || upper.contains("_SECRET")
        || upper.contains("_PASSWORD")
        || upper.contains("_URL")
        || upper.contains("_DB")
        || upper.contains("API_");

    // Exact word checks (Solves the "PASSWORD" problem)
    let is_exact_keyword = matches!(
        upper.as_str(),
        "PASSWORD"
            | "USERNAME"
            | "USER"
            | "HOST"
            | "PORT"
            | "DB"
            | "DATABASE"
            | "TOKEN"
            | "SECRET"
            | "KEY"
            | "ENV"
    );

    has_keyword_suffix || is_exact_keyword
}
fn build_memory_map(segments: &[DataSegment]) -> HashMap<u32, u8> {
    let mut map = HashMap::new();
    for segment in segments {
        for (i, &byte) in segment.data.iter().enumerate() {
            map.insert(segment.offset + i as u32, byte);
        }
    }
    map
}

fn extract_string_from_memory(
    memory_map: &HashMap<u32, u8>,
    pointer: u32,
    length: u32,
) -> Option<String> {
    if length == 0 || length > 1000 {
        return None;
    }

    let mut bytes = Vec::with_capacity(length as usize);
    for offset in pointer..pointer + length {
        bytes.push(*memory_map.get(&offset)?);
    }

    std::str::from_utf8(&bytes).ok().map(|s| s.to_string())
}

fn extract_const_i32(expr: &ConstExpr) -> Option<i32> {
    let mut reader = expr.get_operators_reader();
    while let Ok(op) = reader.read() {
        if let Operator::I32Const { value } = op {
            return Some(value);
        }
    }
    None
}

fn extract_const_offset(expr: &ConstExpr) -> Option<u32> {
    let mut reader = expr.get_operators_reader();
    while let Ok(op) = reader.read() {
        if let Operator::I32Const { value } = op {
            return Some(value as u32);
        }
    }
    None
}

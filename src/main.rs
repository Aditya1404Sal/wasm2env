use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use wasmparser::{ConstExpr, FunctionBody, Operator, Parser, Payload};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: wasm2env <wasm-file>");
        eprintln!();
        eprintln!("Detects environment variables by analyzing WASM bytecode.");
        return Ok(());
    }

    let path = &args[1];
    let data = fs::read(path).with_context(|| format!("Failed to read: {}", path))?;

    println!("🔍 Analyzing WASM module for environment dependencies...");
    println!("File: {}", path);
    println!("---------------------------------------------------\n");

    let env_vars = detect_env_vars(&data)?;

    if env_vars.is_empty() {
        println!("✅ No environment variable dependencies detected.");
    } else {
        println!("📋 Required Environment Variables ({}):\n", env_vars.len());

        for (i, var_name) in env_vars.iter().enumerate() {
            println!("  {}. {}", i + 1, var_name);
        }

        println!("\n💡 Configure these in wasmcloud before deployment.");
    }

    println!("\n---------------------------------------------------");
    Ok(())
}

/// Main detection function
fn detect_env_vars(wasm_bytes: &[u8]) -> Result<HashSet<String>> {
    let parser = Parser::new(0);

    let mut data_segments = Vec::new();
    let mut global_values = Vec::new();
    let mut env_vars = HashSet::new();

    // Single-pass collection
    for payload in parser.parse_all(wasm_bytes) {
        match payload? {
            Payload::DataSection(reader) => {
                collect_data_segments(reader, &mut data_segments)?;
            }
            Payload::GlobalSection(reader) => {
                collect_globals(reader, &mut global_values)?;
            }
            _ => {}
        }
    }

    let memory_map = build_memory_map(&data_segments);

    // Analyze functions
    let parser = Parser::new(0);
    for payload in parser.parse_all(wasm_bytes) {
        if let Ok(Payload::CodeSectionEntry(body)) = payload {
            analyze_function(body, &global_values, &memory_map, &mut env_vars)?;
        }
    }

    // Sort for consistent output
    Ok(env_vars)
}

/// Collect data segments from the data section
fn collect_data_segments(
    reader: wasmparser::DataSectionReader,
    segments: &mut Vec<DataSegment>,
) -> Result<()> {
    for data_entry in reader {
        let data_entry = data_entry?;
        if let wasmparser::DataKind::Active { offset_expr, .. } = data_entry.kind {
            if let Some(offset) = extract_i32_const(&offset_expr) {
                segments.push(DataSegment {
                    offset: offset as u32,
                    data: data_entry.data.to_vec(),
                });
            }
        }
    }
    Ok(())
}

/// Collect global constant values
fn collect_globals(reader: wasmparser::GlobalSectionReader, globals: &mut Vec<i32>) -> Result<()> {
    for global in reader {
        let global = global?;
        if let Some(value) = extract_i32_const(&global.init_expr) {
            globals.push(value);
        }
    }
    Ok(())
}

/// Analyze a single function for env var string references
fn analyze_function(
    body: FunctionBody,
    globals: &[i32],
    memory_map: &HashMap<u32, u8>,
    env_vars: &mut HashSet<String>,
) -> Result<()> {
    let mut reader = body.get_binary_reader();
    let mut frame = StackFrame::new();

    // Skip local declarations
    let local_count = reader.read_var_u32()?;
    for _ in 0..local_count {
        reader.read_var_u32()?;
        reader.read::<wasmparser::ValType>()?;
    }

    // Simulate execution
    while !reader.eof() {
        match reader.read_operator()? {
            // Constants
            Operator::I32Const { value } => {
                frame.push(Value::Known(value));
            }
            Operator::I64Const { .. } | Operator::F32Const { .. } | Operator::F64Const { .. } => {
                frame.push(Value::Unknown);
            }

            // Globals
            Operator::GlobalGet { global_index } => {
                let val = globals
                    .get(global_index as usize)
                    .map(|&v| Value::Known(v))
                    .unwrap_or(Value::Unknown);
                frame.push(val);
            }

            // Locals
            Operator::LocalGet { local_index } => {
                frame.push(frame.get_local(local_index));
            }
            Operator::LocalSet { local_index } => {
                let val = frame.pop();
                frame.set_local(local_index, val);
            }
            Operator::LocalTee { local_index } => {
                let val = frame.peek(0);
                frame.set_local(local_index, val);
            }

            // Arithmetic
            Operator::I32Add => {
                let b = frame.pop();
                let a = frame.pop();
                frame.push(match (a, b) {
                    (Value::Known(x), Value::Known(y)) => Value::Known(x.wrapping_add(y)),
                    _ => Value::Unknown,
                });
            }
            Operator::I32Sub
            | Operator::I32Mul
            | Operator::I32DivS
            | Operator::I32DivU
            | Operator::I32RemS
            | Operator::I32RemU => {
                frame.pop();
                frame.pop();
                frame.push(Value::Unknown);
            }

            // Comparisons
            Operator::I32Eqz => {
                frame.pop();
                frame.push(Value::Unknown);
            }
            Operator::I32Eq
            | Operator::I32Ne
            | Operator::I32LtS
            | Operator::I32LtU
            | Operator::I32GtS
            | Operator::I32GtU
            | Operator::I32LeS
            | Operator::I32LeU
            | Operator::I32GeS
            | Operator::I32GeU => {
                frame.pop();
                frame.pop();
                frame.push(Value::Unknown);
            }

            // Bitwise
            Operator::I32And
            | Operator::I32Or
            | Operator::I32Xor
            | Operator::I32Shl
            | Operator::I32ShrS
            | Operator::I32ShrU
            | Operator::I32Rotl
            | Operator::I32Rotr => {
                frame.pop();
                frame.pop();
                frame.push(Value::Unknown);
            }

            // Memory operations
            Operator::I32Load { .. }
            | Operator::I64Load { .. }
            | Operator::F32Load { .. }
            | Operator::F64Load { .. }
            | Operator::I32Load8S { .. }
            | Operator::I32Load8U { .. }
            | Operator::I32Load16S { .. }
            | Operator::I32Load16U { .. } => {
                frame.pop();
                frame.push(Value::Unknown);
            }
            Operator::I32Store { .. }
            | Operator::I64Store { .. }
            | Operator::F32Store { .. }
            | Operator::F64Store { .. }
            | Operator::I32Store8 { .. }
            | Operator::I32Store16 { .. } => {
                frame.pop();
                frame.pop();
            }

            // Function calls - check for env var patterns
            Operator::Call { .. } | Operator::CallIndirect { .. } => {
                if let (Value::Known(ptr), Value::Known(len)) = (frame.peek(1), frame.peek(0)) {
                    // Strict bounds: ptr in valid memory range, len reasonable
                    if ptr > 0x1000 && len >= 3 && len <= 100 {
                        if let Some(s) = read_string(memory_map, ptr as u32, len as u32) {
                            if is_env_var(&s) {
                                env_vars.insert(s);
                            }
                        }
                    }
                }
                frame.clear();
                frame.push(Value::Unknown);
            }

            // Stack manipulation
            Operator::Drop => {
                frame.pop();
            }
            Operator::Select => {
                frame.pop();
                let b = frame.pop();
                let a = frame.pop();
                frame.push(if a == b { a } else { Value::Unknown });
            }

            // Control flow (simplified - we don't need perfect control flow tracking)
            Operator::Return => break,
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::Else
            | Operator::End
            | Operator::Br { .. }
            | Operator::BrIf { .. }
            | Operator::BrTable { .. }
            | Operator::Unreachable
            | Operator::Nop => {}

            _ => {}
        }
    }

    Ok(())
}

/// Optimized environment variable candidate check
fn is_env_var(s: &str) -> bool {
    let len = s.len();

    // Fast path: length check
    if len < 4 || len > 100 {
        return false;
    }

    let mut letter_count = 0;
    let mut has_underscore = false;
    let mut all_upper_or_underscore = true;

    // Single-pass character analysis
    for ch in s.chars() {
        match ch {
            'A'..='Z' => letter_count += 1,
            'a'..='z' => {
                letter_count += 1;
                all_upper_or_underscore = false;
            }
            '_' => has_underscore = true,
            '0'..='9' => {}
            _ => return false, // Invalid character
        }
    }

    // Must have at least 4 letters
    if letter_count < 4 {
        return false;
    }

    // Letters must be at least 50% of the string
    if letter_count * 2 < len {
        return false;
    }

    // Check for Rust mangling patterns (fast reject)
    if s.as_bytes()[0] == b'_' || s.as_bytes()[len - 1] == b'_' {
        return false;
    }

    // Check common noise patterns (compiled to efficient match)
    if matches!(
        s,
        "HTTP"
            | "HTTPS"
            | "JSON"
            | "UTF8"
            | "WASM"
            | "COMPONENT"
            | "LOCALHOST"
            | "MAIN"
            | "FALSE"
            | "TRUE"
            | "FILE"
    ) {
        return false;
    }

    // Exclude Rust internal patterns
    if s.contains("::") || s.contains("Error") && !has_underscore {
        return false;
    }

    // Exclude stdlib variables
    if s.contains("RUST_") || s.contains("BACKTRACE") {
        return false;
    }

    // Pattern 1: SCREAMING_SNAKE_CASE (most common for env vars)
    if has_underscore && all_upper_or_underscore {
        return true;
    }

    // Pattern 2: Contains strong env var keywords
    let upper = s.to_uppercase();
    has_underscore
        && (upper.contains("_KEY")
            || upper.contains("_TOKEN")
            || upper.contains("_SECRET")
            || upper.contains("_PASSWORD")
            || upper.contains("_URL")
            || upper.contains("_DB")
            || upper.contains("API_KEY")
            || upper.contains("DATABASE_")
            || upper.contains("HOST_")
            || upper.contains("_PORT")
            || upper.contains("_API_"))
}

// ===== Helper Types and Functions =====

#[derive(Debug, Clone)]
struct DataSegment {
    offset: u32,
    data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Known(i32),
    Unknown,
}

struct StackFrame {
    stack: Vec<Value>,
    locals: HashMap<u32, Value>,
}

impl StackFrame {
    fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
            locals: HashMap::new(),
        }
    }

    #[inline]
    fn push(&mut self, val: Value) {
        self.stack.push(val);
    }

    #[inline]
    fn pop(&mut self) -> Value {
        self.stack.pop().unwrap_or(Value::Unknown)
    }

    #[inline]
    fn peek(&self, depth: usize) -> Value {
        self.stack
            .get(self.stack.len().wrapping_sub(depth + 1))
            .copied()
            .unwrap_or(Value::Unknown)
    }

    #[inline]
    fn clear(&mut self) {
        self.stack.clear();
    }

    #[inline]
    fn get_local(&self, index: u32) -> Value {
        self.locals.get(&index).copied().unwrap_or(Value::Unknown)
    }

    #[inline]
    fn set_local(&mut self, index: u32, val: Value) {
        self.locals.insert(index, val);
    }
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

fn extract_i32_const(expr: &ConstExpr) -> Option<i32> {
    let mut reader = expr.get_operators_reader();
    while let Ok(op) = reader.read() {
        if let Operator::I32Const { value } = op {
            return Some(value);
        }
    }
    None
}

# wasm2env

A Rust library and CLI tool that detects environment variable and configuration dependencies in WASM components using call-graph taint analysis — no heuristics, no guessing.

## Features

- **Concrete detection** via WASI import call-graph tracing (not pattern matching)
- Supports `wasi:cli/environment`, `wasi_snapshot_preview1`, and `wasi:config/store`
- Handles WASM Component Model binaries (extracts and analyzes embedded core modules)
- Walks all control flow branches (both arms of if/else, loop bodies, nested blocks)
- Library crate for programmatic usage
- CLI tool for command-line usage
- FFI-friendly API for integration with other languages (Elixir, etc.)

## Installation

### As a library dependency

Add to your `Cargo.toml`:
```toml
[dependencies]
wasm2env = { git = "https://github.com/Aditya1404Sal/wasm2env" }
```

### As a CLI tool
```bash
cargo install --git https://github.com/Aditya1404Sal/wasm2env
```

Or build locally:
```bash
git clone https://github.com/Aditya1404Sal/wasm2env
cd wasm2env
cargo build --release
# Binary will be at: ./target/release/wasm2env
```

## Usage

### Rust Library

#### Scan from file path

```rust
use wasm2env::scan_wasm_file;

fn main() -> anyhow::Result<()> {
    let env_vars = scan_wasm_file("./my-component.wasm")?;
    
    for var in env_vars {
        println!("Required: {}", var);
    }
    
    Ok(())
}
```

#### Scan from bytes (recommended for FFI)

```rust
use wasm2env::scan_wasm_bytes;

fn main() -> anyhow::Result<()> {
    let wasm_data = std::fs::read("./my-component.wasm")?;
    let env_vars = scan_wasm_bytes(&wasm_data)?;
    
    for var in env_vars {
        println!("Required: {}", var);
    }
    
    Ok(())
}
```

### CLI

```bash
wasm2env path/to/component.wasm
```

Output:
```
Analyzing WASM module for environment dependencies...
File: path/to/component.wasm
---------------------------------------------------

Required Environment Variables (3):

  1. DATABASE_URL
  2. API_KEY
  3. SECRET_TOKEN

Configure these in wasmcloud before deployment.

---------------------------------------------------
```

## Elixir Integration

For Elixir codebases, use [Rustler](https://github.com/rusterlium/rustler) to create a NIF.

### Step 1: Add Rustler NIF wrapper

Create `native/wasm2env_nif/src/lib.rs`:

```rust
use rustler::{Encoder, Env, Term};
use wasm2env::scan_wasm_bytes;

#[rustler::nif]
fn scan_wasm(bytes: Vec<u8>) -> Result<Vec<String>, String> {
    scan_wasm_bytes(&bytes)
        .map_err(|e| format!("WASM scan error: {}", e))
}

rustler::init!("Elixir.Wasm2Env.Native", [scan_wasm]);
```

### Step 2: Elixir module

```elixir
defmodule Wasm2Env.Native do
  use Rustler, otp_app: :your_app, crate: "wasm2env_nif"

  # When your NIF is loaded, it will override this function
  def scan_wasm(_bytes), do: :erlang.nif_error(:nif_not_loaded)
end

defmodule Wasm2Env do
  @moduledoc """
  Scans WASM binaries for environment variable dependencies.
  """

  @doc """
  Scans a WASM file for environment variables.

  ## Examples

      iex> Wasm2Env.scan_file("./my_component.wasm")
      {:ok, ["DATABASE_URL", "API_KEY"]}

      iex> Wasm2Env.scan_file("./invalid.wasm")
      {:error, "WASM scan error: ..."}
  """
  def scan_file(path) do
    case File.read(path) do
      {:ok, bytes} -> scan_bytes(bytes)
      {:error, reason} -> {:error, "Failed to read file: #{reason}"}
    end
  end

  @doc """
  Scans WASM binary bytes for environment variables.

  ## Examples

      iex> wasm_bytes = File.read!("./my_component.wasm")
      iex> Wasm2Env.scan_bytes(wasm_bytes)
      {:ok, ["DATABASE_URL", "API_KEY"]}
  """
  def scan_bytes(bytes) when is_binary(bytes) do
    bytes
    |> :binary.bin_to_list()
    |> Wasm2Env.Native.scan_wasm()
    |> case do
      {:ok, env_vars} -> {:ok, env_vars}
      {:error, _} = error -> error
    end
  end
end
```

### Step 3: Usage in Elixir

```elixir
# Scan a WASM file
{:ok, env_vars} = Wasm2Env.scan_file("./my_component.wasm")
IO.inspect(env_vars)
# Output: ["DATABASE_URL", "API_KEY", "SECRET_TOKEN"]

# Or scan bytes directly
wasm_bytes = File.read!("./my_component.wasm")
{:ok, env_vars} = Wasm2Env.scan_bytes(wasm_bytes)
```

## How It Works

The tool uses **call-graph taint analysis** — not heuristic pattern matching — to concretely identify which strings are used with WASI environment/config APIs.

### Pipeline

1. **Extract core modules**: Parses the WASM Component Model envelope (via `wasmparser`) and extracts embedded core modules
2. **Identify WASI imports**: Finds environment-related imports in each core module:
   - `wasi:cli/environment` → `get-environment` (WASI preview2)
   - `wasi_snapshot_preview1` → `environ_get` / `environ_sizes_get` (WASI preview1)
   - `wasi:config/store` → `get` (WASI config store)
3. **Build reverse call graph**: Using `walrus` structured IR, maps every function to the set of functions it calls
4. **Taint propagation**: BFS from the WASI imports through the reverse call graph to find all functions that transitively call any environment/config API
5. **Stack simulation**: Walks the walrus IR for every function, tracking `i32` constants through the operand stack and locals. On `IfElse`, forks the stack state and walks **both branches**
6. **Capture at call sites**: When a `call` instruction targets a function in the tainted set, reads any `(ptr, len)` string arguments from the simulated stack and resolves them against the data segment memory map

A string is reported if and only if it is passed as an argument to a function that ultimately calls a WASI environment or config API. No naming convention assumptions, no keyword matching.

### What it does NOT do

- No heuristic pattern matching (no `SCREAMING_SNAKE_CASE` guessing)
- No dynamic execution (doesn't run the WASM binary)
- No symbolic execution (tracks constants, not symbolic values)

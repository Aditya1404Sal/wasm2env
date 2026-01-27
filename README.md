# wasm2env

A Rust library and CLI tool to detect environment variable dependencies in WASM binaries by analyzing bytecode.

## Features

-  Static analysis of WASM bytecode to detect env var usage
-  Library crate for programmatic usage
-  CLI tool for command-line usage
-  FFI-friendly API for integration with other languages (Elixir, etc.)

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

The library performs static analysis on WASM bytecode:

1. **Parse WASM sections**: Extracts data segments and globals
2. **Simulate execution**: Tracks constant values through the stack
3. **Pattern matching**: Identifies strings matching env var patterns:
   - `SCREAMING_SNAKE_CASE`
   - Keywords: `_KEY`, `_TOKEN`, `_SECRET`, `_PASSWORD`, `_URL`, etc.
4. **Filtering**: Removes false positives (HTTP, LOCALHOST, etc.)

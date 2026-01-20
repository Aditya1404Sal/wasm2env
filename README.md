# Env Var Detector (WASM)

 **Statically detect environment variable dependencies in WebAssembly binaries**

This tool analyzes **WASM bytecode** to identify **environment variables** referenced by a module *without executing it*.
It’s designed for **wasmCloud / component-based runtimes**, where knowing env dependencies ahead of deployment matters.

---

## Why this exists

In WASM deployments (especially wasmCloud):

* Environment variables must be **explicitly configured**
* Missing env vars cause **runtime failures**
* Source code may not be available (3rd-party components, OCI artifacts)

This tool answers a simple but critical question:

> **“Which environment variables does this WASM module expect?”**

---

## How it works (high level)

The detector performs **static analysis** on the WASM binary:

1. Parses the module using `wasmparser`
2. Collects:

   * Data segments (for embedded strings)
   * Global constant values
3. Builds a memory map of static data
4. Simulates function execution:

   * Tracks stack values
   * Propagates constants where possible
   * Detects `(ptr, len)` string arguments passed to function calls
5. Applies heuristics to identify **environment-variable-like strings**

---

## Features

* Static (no WASM execution)
* No source code required
* Handles Rust-compiled WASM reasonably well
* Filters noise (stdlib, Rust internals, common constants)
* Outputs unique env vars only
* wasmCloud-friendly output

---

## Usage

```bash
./wasm2env -- <path-to-wasm-file>
```

### Example

```bash
./wasm2env openai_component.wasm
```

Output:

```text
🔍 Analyzing WASM module for environment dependencies...
File: my_component.wasm
---------------------------------------------------

📋 Required Environment Variables (3):

  1. DATABASE_URL
  2. API_KEY
  3. SERVICE_PORT

💡 Configure these in wasmcloud before deployment.

---------------------------------------------------
```

---

## Environment Variable Detection Heuristics

A string is considered an env var candidate if:

* Length is between **4–100 characters**
* Contains mostly letters
* Uses valid env-var characters (`A–Z`, `a–z`, `0–9`, `_`)
* Matches common patterns:

  * `SCREAMING_SNAKE_CASE`
  * `_KEY`, `_TOKEN`, `_SECRET`, `_URL`, `_PORT`, etc.
* Excludes:

  * Rust internals (`RUST_`, `BACKTRACE`)
  * Common constants (`HTTP`, `JSON`, `TRUE`, `FALSE`)
  * Mangled or invalid identifiers

This keeps false positives low while catching real deployment requirements.

---

## Limitations (by design)

* Not a full WASM interpreter
* Control flow is approximated
* Dynamic string construction at runtime may not be detected
* Obfuscated or encrypted strings won’t show up

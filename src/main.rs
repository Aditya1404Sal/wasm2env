use anyhow::Result;
use wasm2env::scan_wasm_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: wasm2env <wasm-file>");
        eprintln!();
        eprintln!("Detects environment variables by analyzing WASM bytecode.");
        return Ok(());
    }

    let path = &args[1];

    println!("Analyzing WASM module for environment dependencies...");
    println!("File: {}", path);
    println!("---------------------------------------------------\n");

    let env_vars = scan_wasm_file(path)?;

    if env_vars.is_empty() {
        println!("No environment variable dependencies detected.");
    } else {
        println!("Required Environment Variables ({}):\n", env_vars.len());

        for (i, var_name) in env_vars.iter().enumerate() {
            println!("  {}. {}", i + 1, var_name);
        }

        println!("\nConfigure these in wasmcloud before deployment.");
    }

    println!("\n---------------------------------------------------");
    Ok(())
}

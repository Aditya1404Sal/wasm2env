#!/usr/bin/env bash
# Build all test component WASM fixtures.
# Usage: ./scripts/build-test-fixtures.sh
#
# Requires: wasm32-wasip2 target installed
#   rustup target add wasm32-wasip2

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TC_DIR="$ROOT_DIR/test-components"

echo "Building test component WASM fixtures..."

# std::env::var components (binary crates — output keeps hyphens)
for component in single-env multi-env no-env conditional-env nested-calls many-vars env-with-digits scale-env; do
  dir="$TC_DIR/$component"
  if [[ ! -d "$dir" ]]; then
    echo "  SKIP $component (not found)"
    continue
  fi
  echo "  BUILD $component"
  cargo build --release --target wasm32-wasip2 --manifest-path "$dir/Cargo.toml" --quiet
  # Binary crates keep hyphens in output name
  cp "$dir/target/wasm32-wasip2/release/$component.wasm" "$TC_DIR/$component.wasm"
done

# wasi:config/store components (cdylib crates — output uses underscores)
for component in config-single config-multi config-none config-conditional config-many config-and-env config-nested false-positive-resistance; do
  dir="$TC_DIR/$component"
  if [[ ! -d "$dir" ]]; then
    echo "  SKIP $component (not found)"
    continue
  fi
  echo "  BUILD $component"
  cargo build --release --target wasm32-wasip2 --manifest-path "$dir/Cargo.toml" --quiet
  # cdylib crates convert hyphens to underscores in output name
  wasm_name="${component//-/_}"
  cp "$dir/target/wasm32-wasip2/release/$wasm_name.wasm" "$TC_DIR/$component.wasm"
done

echo "Done. Built fixtures:"
ls -lh "$TC_DIR"/*.wasm

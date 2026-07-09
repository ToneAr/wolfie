#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

if ! command -v evcxr >/dev/null 2>&1; then
  echo "evcxr is not installed. Install it with: cargo install --locked evcxr_repl" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is not installed or not on PATH." >&2
  exit 1
fi

cargo build

OUT_DIR_FOUND=""
for candidate in target/debug/build/wolfram-cli-*/out; do
  if [ -f "$candidate/builtin_symbols.tsv" ]; then
    OUT_DIR_FOUND="$SCRIPT_DIR/$candidate"
    break
  fi
done

if [ -z "$OUT_DIR_FOUND" ]; then
  echo "Could not find generated builtin_symbols.tsv under target/debug/build/wolfram-cli-*/out." >&2
  echo "Try running cargo build and check the build output." >&2
  exit 1
fi

export XDG_CONFIG_HOME="$SCRIPT_DIR/.evcxr"
export OUT_DIR="$OUT_DIR_FOUND"
export WOLFRAM_CLI_ROOT="$SCRIPT_DIR"

exec evcxr "$@"

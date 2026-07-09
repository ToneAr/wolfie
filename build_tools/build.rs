use std::{env, fs, path::PathBuf};

const BUILTIN_SYMBOLS: &[u8] = include_bytes!("builtin_symbols.tsv");

fn main() {
    println!("cargo:rerun-if-changed=build_tools/builtin_symbols.tsv");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let output_path = out_dir.join("builtin_symbols.tsv");
    fs::write(&output_path, BUILTIN_SYMBOLS).expect("failed to write builtin symbol table");
}

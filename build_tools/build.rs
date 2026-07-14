use std::{env, fs, path::PathBuf};

const BUILTIN_SYMBOLS: &[u8] = include_bytes!("builtin_symbols.tsv");

fn main() {
    println!("cargo:rerun-if-changed=build_tools/builtin_symbols.tsv");
    println!("cargo:rerun-if-env-changed=WOLFIE_BUILD_UID");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");

    let system_id = wolfram_system_id();
    let build_uid = env::var("WOLFIE_BUILD_UID")
        .or_else(|_| env::var("GITHUB_RUN_ID"))
        .unwrap_or_else(|_| "local".to_string());
    println!("cargo:rustc-env=WOLFIE_SYSTEM_ID={system_id}");
    println!("cargo:rustc-env=WOLFIE_BUILD_UID={build_uid}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let output_path = out_dir.join("builtin_symbols.tsv");
    fs::write(&output_path, BUILTIN_SYMBOLS).expect("failed to write builtin symbol table");
}

fn wolfram_system_id() -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    let target_arch =
        env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture is set by Cargo");

    match (target_os.as_str(), target_arch.as_str()) {
        ("linux", "x86_64") => "Linux-x86-64".to_string(),
        ("macos", "x86_64") => "MacOSX-x86-64".to_string(),
        ("macos", "aarch64") => "MacOSX-ARM64".to_string(),
        ("windows", "x86_64") => "Windows-x86-64".to_string(),
        _ => format!("{target_os}-{target_arch}"),
    }
}

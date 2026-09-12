//! Records the compiler that built the driver, so the report can name the
//! toolchain alongside the core revision.
fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=AMBER_CORE_OPS_RUSTC={version}");
    println!("cargo:rerun-if-changed=build.rs");
}

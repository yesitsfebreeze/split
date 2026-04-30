use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=plugins/rs/src");
    println!("cargo:rerun-if-changed=plugins/rs/Cargo.toml");
    println!("cargo:rerun-if-changed=plugins/py/src");
    println!("cargo:rerun-if-changed=plugins/py/Cargo.toml");

    build_plugin("rs", "split_plugin_rs");
    build_plugin("py", "split_plugin_py");
}

fn build_plugin(lang: &str, crate_name: &str) {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dst = format!("{out_dir}/{crate_name}.wasm");
    let manifest_str = format!("plugins/{lang}/Cargo.toml");
    let manifest = Path::new(&manifest_str);

    for target in ["wasm32-wasip1", "wasm32-wasi"] {
        let ok = Command::new("cargo")
            .args(["build", "--target", target, "--release", "--manifest-path"])
            .arg(manifest)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            let src = format!(
                "plugins/{lang}/target/{target}/release/{crate_name}.wasm"
            );
            if std::fs::copy(&src, &dst).is_ok() {
                return;
            }
        }
    }

    std::fs::write(&dst, b"").unwrap();
    println!(
        "cargo:warning=wasm32-wasip1 target not found; {lang} plugin falls back to native splitter"
    );
    println!("cargo:warning=Install with: rustup target add wasm32-wasip1");
}

include!("src/command_names.rs");

use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    llama_server()?;
    // The settings window's commands and the overlay's (U4).
    let commands: &'static [&'static str] = Box::leak(
        COMMAND_NAMES
            .iter()
            .chain(OVERLAY_COMMAND_NAMES)
            .copied()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let manifest = tauri_build::AppManifest::new().commands(commands);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))?;
    Ok(())
}

/// On macOS the app bundles the cleanup helper (Q4, `tauri.macos.conf.json`
/// `externalBin`), and `tauri_build` refuses to build without it, so build
/// it here when it is missing: `scripts/build-llama-server.sh` downloads the
/// pinned llama.cpp source, checks its SHA-256 and builds it once per
/// machine (cached), then copies it in.
fn llama_server() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../scripts/build-llama-server.sh");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return Ok(());
    }
    let triple = std::env::var("TARGET")?;
    let binary = format!("binaries/llama-server-{triple}");
    println!("cargo:rerun-if-changed={binary}");
    if Path::new(&binary).is_file() {
        return Ok(());
    }
    let status = Command::new("../scripts/build-llama-server.sh")
        .arg(&triple)
        .status()?;
    if !status.success() {
        return Err(format!(
            "scripts/build-llama-server.sh {triple} failed ({status}); the app bundles its \
             llama-server helper on macOS"
        )
        .into());
    }
    Ok(())
}

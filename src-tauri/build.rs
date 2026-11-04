include!("src/command_names.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

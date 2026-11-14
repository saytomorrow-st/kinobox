use tauri_build::{AppManifest, Attributes};

const COMMANDS: &[&str] = &[
    "boot",
    "login",
    "logout",
    "session",
    "mirrors",
    "check_mirrors",
    "add_mirror",
    "select_mirror",
    "quit",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let attributes = Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes)?;
    Ok(())
}

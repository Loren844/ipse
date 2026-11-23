//! Tauri's build step: the app's configuration, icons and permissions; and the memory
//! format this version reads, from the core, for updates (`src/updates.rs`).

use std::path::Path;

/// Where the core says its memory format.
const SETTINGS: &str = "../crates/ipse-core/src/vault/settings.rs";

fn main() {
    println!("cargo::rerun-if-changed={SETTINGS}");
    let text = std::fs::read_to_string(Path::new(SETTINGS))
        .unwrap_or_else(|error| panic!("{SETTINGS}: {error}"));
    let format = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("pub const FORMAT: u32 = "))
        .and_then(|rest| rest.strip_suffix(';'))
        .unwrap_or_else(|| panic!("{SETTINGS} says no `pub const FORMAT: u32`"));
    println!("cargo::rustc-env=IPSE_MEMORY_FORMAT={format}");
    tauri_build::build();
}

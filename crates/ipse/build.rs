//! Embeds the built interface (`ui/build/`, committed) in the binary, so that
//! `cargo install` needs no Node and `ipse ui` serves it without any other file
//! (decision 0013). Also reads the hash of the page's inline start-up script, for the
//! Content-Security-Policy.

// A build script fails the build by panicking, with a message saying what to do.
#![allow(clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let build = manifest.join("../../ui/build");
    println!("cargo:rerun-if-changed={}", build.display());

    let mut files = Vec::new();
    collect(&build, &build, &mut files);
    files.sort();

    let mut out = String::from("/// The built interface: path, content, media type.\n");
    out.push_str("pub(crate) static ASSETS: &[(&str, &[u8], &str)] = &[\n");
    for (rel, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(
            out,
            "    ({rel:?}, include_bytes!({:?}), {:?}),",
            path.display().to_string(),
            media_type(rel)
        );
    }
    out.push_str("];\n");

    let Ok(index) = std::fs::read_to_string(build.join("index.html")) else {
        panic!("ui/build/index.html is missing: run `npm ci && npm run build` in ui/");
    };
    let script_src = index
        .split("script-src ")
        .nth(1)
        .and_then(|rest| rest.split(['"', ';']).next())
        .unwrap_or("'self'")
        .trim()
        .to_owned();
    let _ = writeln!(
        out,
        "/// The scripts the page may run: its own files and its start-up script.\npub(crate) const SCRIPT_SRC: &str = {script_src:?};"
    );

    let target =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("set by cargo")).join("ui_assets.rs");
    std::fs::write(target, out).expect("OUT_DIR is writable");
}

/// Every file under `dir`, with its path relative to `root` and `/` separators.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((rel, path));
        }
    }
}

fn media_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

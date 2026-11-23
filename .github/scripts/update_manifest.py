"""Writes `latest.json`, the manifest the desktop app reads to update itself (issue #156).

The app asks `releases/latest/download/latest.json`: the version, its notes (the
changelog's section), and for each system the address of the signed file and its
signature (`.sig`, made by Tauri's build with the release's signing key). It also says
the memory format of the version (`FORMAT` in `crates/ipse-core/src/vault/settings.rs`),
so that the app warns before installing a version that changes it.

Usage: update_manifest.py <folder of the files and their .sig> <tag> <repository>
       <CHANGELOG.md> <settings.rs>; prints the manifest.
"""

import datetime
import json
import os
import re
import sys

# The suffix of each system's file, as app-release.yml names them, and the platforms
# Tauri's updater looks for (the plain one, then the one of the installer's kind).
SYSTEMS = [
    ("macos-apple-silicon.app.tar.gz", ["darwin-aarch64", "darwin-aarch64-app"]),
    ("macos-intel.app.tar.gz", ["darwin-x86_64", "darwin-x86_64-app"]),
    ("windows-x64.msi", ["windows-x86_64", "windows-x86_64-msi"]),
    ("linux-x64.AppImage", ["linux-x86_64", "linux-x86_64-appimage"]),
    # Installed from the .deb, the app updates through dpkg, which asks for the password.
    ("linux-x64.deb", ["linux-x86_64-deb"]),
]


def section(changelog: str, version: str) -> str:
    """The changelog's section of `version`, without its heading; empty when none."""
    match = re.search(
        rf"^## {re.escape(version)}\b[^\n]*\n(.*?)(?=^## |\Z)", changelog, flags=re.M | re.S
    )
    return match.group(1).strip() if match else ""


def memory_format(settings: str) -> int:
    """The memory format the core reads and writes."""
    match = re.search(r"^pub const FORMAT: u32 = (\d+);", settings, flags=re.M)
    if not match:
        raise SystemExit("settings.rs says no `pub const FORMAT: u32`")
    return int(match.group(1))


def manifest(folder: str, tag: str, repository: str, changelog: str, settings: str) -> dict:
    version = tag.removeprefix("v")
    base = f"https://github.com/{repository}/releases/download/{tag}"
    platforms = {}
    for suffix, names in SYSTEMS:
        name = f"Ipse-{version}-{suffix}"
        signature = os.path.join(folder, f"{name}.sig")
        if not os.path.isfile(signature):
            continue
        with open(signature, encoding="utf-8") as file:
            entry = {"signature": file.read().strip(), "url": f"{base}/{name}"}
        for platform in names:
            platforms[platform] = entry
    if not platforms:
        raise SystemExit(f"no signed file in {folder}")
    return {
        "version": version,
        "notes": section(changelog, version),
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "memory_format": memory_format(settings),
        "platforms": platforms,
    }


if __name__ == "__main__":
    folder, tag, repository, changelog_path, settings_path = sys.argv[1:6]
    with open(changelog_path, encoding="utf-8") as file:
        changelog = file.read()
    with open(settings_path, encoding="utf-8") as file:
        settings = file.read()
    print(json.dumps(manifest(folder, tag, repository, changelog, settings), indent=2, ensure_ascii=False))

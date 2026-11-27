"""Puts the desktop app first in a release's notes (issue #168).

cargo-dist writes the notes: the changelog section, then how to install the `ipse`
command. This adds, at the top, one link per system to the app's installers attached
to the release, and names cargo-dist's section for what it is. Running it again
replaces the app's section: it is safe to repeat.

The installers that are not signed (issue #257) get a word on opening them the first
time. `SIGNED` names the systems whose installers are signed, `macos` and `windows`,
separated by spaces; app-release.yml sets it. Unset, as when rewriting an old
release's notes, neither is.

Usage: release_notes.py <body file> <tag> <repository>; prints the new body.
"""

import os
import re
import sys

START = "<!-- ipse-app -->"
END = "<!-- /ipse-app -->"

SYSTEMS = [
    ("Mac with an Apple chip (M1 and later)", "macos-apple-silicon.dmg"),
    ("Mac with an Intel chip", "macos-intel.dmg"),
    ("Windows 10 or 11", "windows-x64.msi"),
    ("Linux: Debian, Ubuntu", "linux-x64.deb"),
    ("Linux: other distributions", "linux-x64.AppImage"),
]


def unsigned_note(signed: set[str], guide: str) -> str:
    """How to open the installers that are not signed; nothing when both are."""
    steps = []
    if "macos" not in signed:
        steps.append(
            "macOS asks you to open Ipse from "
            "**System Settings → Privacy & Security → Open Anyway**"
        )
    if "windows" not in signed:
        steps.append("Windows from **More info → Run anyway**")
    if not steps:
        return ""
    which = "The installers are" if len(steps) == 2 else (
        "The Mac installers are" if "macos" not in signed else "The Windows installer is"
    )
    if len(steps) == 1 and "windows" not in signed:
        steps[0] = steps[0].replace("Windows from", "Windows asks you to run it from")
    return (
        f"{which} not signed yet: the first time, {', and '.join(steps)}. "
        f"[Install Ipse]({guide}) shows each step.\n"
    )


def notes(body: str, tag: str, repository: str, signed: set[str] = frozenset()) -> str:
    version = tag.removeprefix("v")
    base = f"https://github.com/{repository}/releases/download/{tag}"
    guide = f"https://github.com/{repository}/blob/{tag}/docs/guide/install.md"
    rows = "\n".join(
        f"| {system} | [Ipse-{version}-{suffix}]({base}/Ipse-{version}-{suffix}) |"
        for system, suffix in SYSTEMS
    )
    app = (
        f"{START}\n## Install the app\n\n"
        "Download the file for your system and open it. The app comes with the `ipse` "
        "command your AI apps start.\n\n"
        f"| System | File |\n| --- | --- |\n{rows}\n\n"
        f"{unsigned_note(signed, guide)}{END}\n\n"
    )
    body = re.sub(re.escape(START) + r".*?" + re.escape(END) + r"\n*", "", body, flags=re.S)
    # cargo-dist's section installs the command line, not the app.
    body = re.sub(
        r"^## Install ipse [^\n]*$",
        "## The ipse command, for the terminal",
        body,
        flags=re.M,
    )
    return app + body


if __name__ == "__main__":
    path, tag, repository = sys.argv[1:4]
    with open(path, encoding="utf-8") as file:
        signed = set(os.environ.get("SIGNED", "").split())
        print(notes(file.read(), tag, repository, signed), end="")

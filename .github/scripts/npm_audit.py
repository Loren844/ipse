"""Fails when a dependency of the interface has a known advisory (issue #285).

Runs `npm audit --json` in the folder given, and fails on every advisory that is not
listed in ALLOWED with the reason it does not reach Ipse. npm has no list of its own
for that, and failing on every advisory would block on one that cannot apply.

Usage: npm_audit.py <folder>
"""

import json
import subprocess
import sys

# Advisory id -> why it does not reach Ipse. Remove an entry with its cause.
ALLOWED = {
    # maplibre-gl <= 6.4.0: DOM.sanitize() can be bypassed. MapLibre sanitizes only
    # the HTML of popups and of the attribution control. Ipse shows neither
    # (`attributionControl: false`, no Popup) and writes its map style itself
    # (ui/src/lib/mapStyle.ts). The fix is in maplibre-gl 6, a major version.
    "GHSA-jrc7-96c5-q579": "no popup or attribution HTML in Ipse's map",
}


def main() -> int:
    folder = sys.argv[1]
    run = subprocess.run(
        ["npm", "audit", "--json"], cwd=folder, capture_output=True, text=True, check=False
    )
    report = json.loads(run.stdout or "{}")
    if "error" in report:
        print(f"npm audit failed: {report['error'].get('summary', report['error'])}")
        return 1
    found = []
    for name, vulnerability in report.get("vulnerabilities", {}).items():
        for via in vulnerability.get("via", []):
            # A string names another package whose own advisory is listed apart.
            if not isinstance(via, dict):
                continue
            advisory = via.get("url", "").rsplit("/", 1)[-1]
            if advisory in ALLOWED:
                print(f"allowed  {advisory}  {name}: {ALLOWED[advisory]}")
            else:
                found.append(f"{advisory}  {name} {via.get('range', '')}: {via.get('title', '')}")
    for line in found:
        print(f"ADVISORY {line}")
    if found:
        print("Update the package, or list the advisory in .github/scripts/npm_audit.py with the reason it does not reach Ipse.")
        return 1
    print("No advisory but the allowed ones.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

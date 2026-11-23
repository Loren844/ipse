#!/usr/bin/env bash
# The test of an update (issue #156), on Linux: an old version of the app, built with
# the `update-test` feature, finds the new version in a manifest served on this machine,
# downloads it, verifies its signature and replaces its own AppImage. The key is made
# for the test and thrown away; the release's key never leaves GitHub's secrets.
#
# Usage, from the repository's root: .github/scripts/update_test.sh
# Needs Node (for Tauri's command line), Python and xvfb-run.
set -euo pipefail

root="$(pwd)"
work="$(mktemp -d)"
trap 'kill "${server:-}" 2> /dev/null || true; rm -rf "$work"' EXIT
target=x86_64-unknown-linux-gnu
tauri="npx --yes @tauri-apps/cli@2"
bundle="$root/app/target/$target/release/bundle/appimage"
port=8765

echo "A throwaway signing key."
(cd app && $tauri signer generate --ci -p test -w "$work/key" > /dev/null)
export TAURI_SIGNING_PRIVATE_KEY="$work/key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=test
pubkey="$(cat "$work/key.pub")"

echo "The ipse command, beside the app."
cargo build --release -p ipse --target "$target"
mkdir -p app/binaries
cp "target/$target/release/ipse" "app/binaries/ipse-$target"

echo "The new version, signed for updates."
rm -rf "$bundle"
signed="$(python3 -c 'import json, sys; print(json.dumps({"bundle": {"createUpdaterArtifacts": True},
  "plugins": {"updater": {"pubkey": sys.argv[1]}}}))' "$pubkey")"
(cd app && $tauri build --target "$target" --bundles appimage --config tauri.bundle.conf.json \
  --config "$signed")
mkdir -p "$work/serve"
new="$(ls "$bundle"/*.AppImage)"
cp "$new" "$work/serve/new.AppImage"
version="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' app/tauri.conf.json)"
python3 - "$work/serve" "$version" "$(cat "$new.sig")" "$port" << 'EOF'
import json, sys
folder, version, signature, port = sys.argv[1:5]
entry = {"signature": signature, "url": f"http://127.0.0.1:{port}/new.AppImage"}
with open(f"{folder}/latest.json", "w") as file:
    json.dump({"version": version, "notes": "The test's update.", "memory_format": 1,
               "platforms": {"linux-x86_64": entry, "linux-x86_64-appimage": entry}}, file)
EOF

echo "The old version, which asks this machine for its updates."
rm -rf "$bundle"
config="$(python3 -c 'import json, sys; print(json.dumps({"version": "0.0.1", "plugins": {"updater": {
  "pubkey": sys.argv[1], "endpoints": [f"http://127.0.0.1:{sys.argv[2]}/latest.json"],
  "dangerousInsecureTransportProtocol": True, "requireSignedVersion": True}}}))' "$pubkey" "$port")"
(cd app && $tauri build --target "$target" --bundles appimage --features update-test \
  --config tauri.bundle.conf.json --config "$config")
cp "$(ls "$bundle"/*.AppImage)" "$work/Ipse.AppImage"
chmod +x "$work/Ipse.AppImage"

python3 -m http.server "$port" --bind 127.0.0.1 --directory "$work/serve" > /dev/null 2>&1 &
server=$!
sleep 1

echo "The old version updates itself."
export HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config"
mkdir -p "$HOME"
status=0
APPIMAGE_EXTRACT_AND_RUN=1 timeout 300 xvfb-run -a "$work/Ipse.AppImage" || status=$?
if [ "$status" -ne 0 ]; then
  echo "The old version did not install the update (exit $status)."
  exit 1
fi
if ! cmp -s "$work/Ipse.AppImage" "$work/serve/new.AppImage"; then
  echo "The old version said it installed the update, but its AppImage is not the new one."
  exit 1
fi
echo "The old version became $version."

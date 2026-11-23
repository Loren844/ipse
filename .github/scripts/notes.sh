#!/usr/bin/env bash
# Rewrites the notes of the release <tag> with the app first (release_notes.py).
set -euo pipefail
tag="$1"
here="$(dirname "$0")"
id=$(gh api "repos/$GITHUB_REPOSITORY/releases/tags/$tag" --jq .id)
gh api "repos/$GITHUB_REPOSITORY/releases/$id" --jq .body > "$RUNNER_TEMP/body.md"
python3 "$here/release_notes.py" "$RUNNER_TEMP/body.md" "$tag" "$GITHUB_REPOSITORY" > "$RUNNER_TEMP/notes.md"
gh api -X PATCH "repos/$GITHUB_REPOSITORY/releases/$id" -F "body=@$RUNNER_TEMP/notes.md" > /dev/null
echo "The notes of $tag start with the app's installers."

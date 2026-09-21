#!/usr/bin/env bash
set -euo pipefail

index="${1:?usage: index-add-version.sh <repository.index> <vX.Y.Z>}"
tag="${2:?usage: index-add-version.sh <repository.index> <vX.Y.Z>}"
cli="${PACT_PLUGIN_CLI:?set PACT_PLUGIN_CLI to the pact-plugin-cli binary}"
repo_url=https://github.com/ZirekHQ/pact-avro-plugin
entry_header='^\[entries\.[^].]+\]$'

if ! [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "::error::'${tag}' is not a vX.Y.Z tag" >&2
  exit 1
fi
version="${tag#v}"

if grep -A1 -Fx '[[entries.avro.versions]]' "$index" | grep -Fxq "version = \"${version}\""; then
  echo present
  exit 0
fi

header() { awk -v re="$entry_header" '$0 ~ re { exit } { print }' "$1"; }
avro_block() { awk -v re="$entry_header" '$0 ~ re { on = ($0 == "[entries.avro]") } on' "$1"; }
replace_avro() {
  awk -v re="$entry_header" -v blk="$2" '
    $0 ~ re { started = 1; skip = ($0 == "[entries.avro]")
              if (skip) { while ((getline l < blk) > 0) print l; replaced = 1 } }
    started && !skip { print }
    END { if (!replaced) while ((getline l < blk) > 0) print l }' "$1"
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cp "$index" "$tmp/repository.index"
cp "$index.sha256" "$tmp/repository.index.sha256"

# Test seam: a local manifest replaces the GitHub release lookup.
if [[ -n "${INDEX_MANIFEST_FILE:-}" ]]; then
  "$cli" repository add-plugin-version file "$tmp/repository.index" "$INDEX_MANIFEST_FILE" >&2
else
  "$cli" repository add-plugin-version git-hub "$tmp/repository.index" "$repo_url/releases/tag/$tag" >&2
fi

avro_block "$tmp/repository.index" > "$tmp/avro.block"
# The CLI reorders every entry on write; keep the original file and take only the header and avro entry from its output.
{ header "$tmp/repository.index"; replace_avro "$index" "$tmp/avro.block"; } > "$tmp/spliced"
cp "$tmp/spliced" "$index"
printf '%s' "$(sha256sum "$index" | cut -d' ' -f1)" > "$index.sha256"
"$cli" repository validate "$index" >&2
echo added

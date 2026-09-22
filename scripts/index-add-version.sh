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

header() {
  local file="$1"
  awk -v re="$entry_header" '$0 ~ re { exit } { print }' "$file"
}
avro_block() {
  local file="$1"
  awk -v re="$entry_header" '$0 ~ re { on = ($0 == "[entries.avro]") } on' "$file"
}
replace_avro() {
  local file="$1" block="$2"
  awk -v re="$entry_header" -v blk="$block" '
    $0 ~ re { started = 1; skip = ($0 == "[entries.avro]")
              if (skip) { while ((getline l < blk) > 0) print l; replaced = 1 } }
    started && !skip { print }
    END { if (!replaced) while ((getline l < blk) > 0) print l }' "$file"
}

add_plugin_version_from_local_test_manifest() {
  "$cli" repository add-plugin-version file "$tmp/repository.index" "$INDEX_MANIFEST_FILE" >&2
}
add_plugin_version_from_github_release() {
  "$cli" repository add-plugin-version git-hub "$tmp/repository.index" "$repo_url/releases/tag/$tag" >&2
}
merge_avro_update_without_reordering_other_entries() {
  header "$tmp/repository.index"
  replace_avro "$index" "$tmp/avro.block"
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cp "$index" "$tmp/repository.index"
cp "$index.sha256" "$tmp/repository.index.sha256"

if [[ -n "${INDEX_MANIFEST_FILE:-}" ]]; then
  add_plugin_version_from_local_test_manifest
else
  add_plugin_version_from_github_release
fi

avro_block "$tmp/repository.index" > "$tmp/avro.block"
merge_avro_update_without_reordering_other_entries > "$tmp/spliced"
cp "$tmp/spliced" "$index"
printf '%s' "$(sha256sum "$index" | cut -d' ' -f1)" > "$index.sha256"
"$cli" repository validate "$index" >&2
echo added

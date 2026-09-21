#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! gh auth status >/dev/null 2>&1 && [[ -z "${GH_TOKEN:-}" ]]; then
  echo "skip: no GitHub credentials for gh"
  exit 0
fi

bash "$here/fetch-pact-plugin-cli.sh" "$tmp/bin"
export PACT_PLUGIN_CLI="$tmp/bin/pact-plugin-cli"
cli="$PACT_PLUGIN_CLI"

index="$tmp/repository.index"
"$cli" repository new "$index" >/dev/null
[[ -f "$index.sha256" ]]

bash "$here/render-release-files.sh" 1.2.3 "$tmp/rendered" manifest
export INDEX_MANIFEST_FILE="$tmp/rendered/pact-plugin.json"

sed 's/"avro"/"other"/; s/pact-avro-plugin/pact-other-plugin/' "$INDEX_MANIFEST_FILE" > "$tmp/other.json"
"$cli" repository add-plugin-version file "$index" "$tmp/other.json" >/dev/null
entry() {
  local file="$1" name="$2"
  awk -v want="[entries.${name}]" '/^\[entries\.[^].]+\]$/ { on = ($0 == want) } on' "$file"
}
other_before="$(entry "$index" other)"
[[ -n "$other_before" ]]

[[ "$(bash "$here/index-add-version.sh" "$index" v1.2.3)" == added ]]
[[ "$other_before" == "$(entry "$index" other)" ]]
"$cli" repository list-versions "$index" avro | grep -q '1\.2\.3'
expected="$(cut -d' ' -f1 "$index.sha256")"
actual="$(sha256sum "$index" | cut -d' ' -f1)"
[[ "$expected" == "$actual" ]]

before="$(sha256sum "$index")"
[[ "$(bash "$here/index-add-version.sh" "$index" v1.2.3)" == present ]]
[[ "$before" == "$(sha256sum "$index")" ]]

bash "$here/render-release-files.sh" 1.2.4 "$tmp/rendered2" manifest
INDEX_MANIFEST_FILE="$tmp/rendered2/pact-plugin.json" bash "$here/index-add-version.sh" "$index" v1.2.4 >/dev/null
"$cli" repository list-versions "$index" avro | grep -q '1\.2\.3'
"$cli" repository list-versions "$index" avro | grep -q '1\.2\.4'

if bash "$here/index-add-version.sh" "$index" 1.2.5 2>/dev/null; then
  echo "expected a tag without the v prefix to be rejected" >&2
  exit 1
fi
echo "index-add-version tests passed"

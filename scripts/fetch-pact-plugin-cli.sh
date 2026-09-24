#!/usr/bin/env bash
set -euo pipefail

dest="${1:?usage: fetch-pact-plugin-cli.sh <dest-dir>}"
repo=pact-foundation/pact-plugins
asset=pact-plugin-linux-x86_64.gz

latest_pact_plugin_cli_tag() {
  gh release list --repo "$repo" --limit 100 --json tagName \
    --jq '[.[].tagName | select(startswith("pact-plugin-cli-v"))][0] // empty'
}

tag="${PACT_PLUGIN_CLI_TAG:-$(latest_pact_plugin_cli_tag)}"
if [[ -z "$tag" ]]; then
  echo "::error::no pact-plugin-cli release found in ${repo} (its \"latest\" release is a driver release with no assets; the CLI has its own pact-plugin-cli-v* tag series)" >&2
  exit 1
fi

mkdir -p "$dest"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

gh release download "$tag" --repo "$repo" --pattern "$asset" --pattern "$asset.sha256" --dir "$tmp"
(cd "$tmp" && sha256sum -c "$asset.sha256")
gunzip -c "$tmp/$asset" > "$dest/pact-plugin-cli"
chmod +x "$dest/pact-plugin-cli"

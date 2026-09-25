#!/usr/bin/env bash
set -euo pipefail

tag="${1:?usage: smoke-install.sh <vX.Y.Z>}"
version="${tag#v}"
repo=https://github.com/ZirekHQ/pact-avro-plugin
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

bash "$(dirname "${BASH_SOURCE[0]}")/fetch-pact-plugin-cli.sh" "$tmp"

check_install() {
  local root="$1"
  local bin="$root/avro-$version/pact-avro-plugin"
  [[ "$("$bin" --version)" == "$version" ]]
  python3 -c 'import json,sys; assert json.load(open(sys.argv[1]))["version"] == sys.argv[2]' \
    "$root/avro-$version/pact-plugin.json" "$version"
}

PACT_PLUGIN_DIR="$tmp/cli" "$tmp/pact-plugin-cli" -y install "$repo/releases/tag/$tag"
check_install "$tmp/cli"

curl --proto '=https' --tlsv1.2 -fsSL "$repo/releases/download/$tag/install-plugin.sh" -o "$tmp/install-plugin.sh"
curl --proto '=https' --tlsv1.2 -fsSL "$repo/releases/download/$tag/install-plugin.sh.sha256" -o "$tmp/install-plugin.sh.sha256"
(cd "$tmp" && sha256sum -c install-plugin.sh.sha256)
PACT_PLUGIN_DIR="$tmp/script" sh "$tmp/install-plugin.sh"
check_install "$tmp/script"

first_line="$(timeout 5 "$tmp/cli/avro-$version/pact-avro-plugin" | head -1 || true)"
echo "$first_line" | python3 -c 'import json,sys; d=json.loads(sys.stdin.read()); assert d["port"] and d["serverKey"]'
echo "smoke test passed for $tag"

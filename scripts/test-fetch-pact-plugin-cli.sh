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
[[ -x "$tmp/bin/pact-plugin-cli" ]]
"$tmp/bin/pact-plugin-cli" --version | grep -q '^pact-plugin-cli [0-9]'

if PACT_PLUGIN_CLI_TAG=pact-plugin-cli-v0.0.0-missing bash "$here/fetch-pact-plugin-cli.sh" "$tmp/bad" 2>/dev/null; then
  echo "expected an unknown CLI tag to fail" >&2
  exit 1
fi
echo "fetch-pact-plugin-cli tests passed"

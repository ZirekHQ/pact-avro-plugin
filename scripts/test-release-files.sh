#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

bash "$here/render-release-files.sh" 1.2.3 "$tmp/out" all

grep -q '"version": "1.2.3"' "$tmp/out/pact-plugin.json"
if grep -q '@VERSION@' "$tmp/out/pact-plugin.json" "$tmp/out/install-plugin.sh"; then
  echo "unrendered @VERSION@ placeholder left in output" >&2
  exit 1
fi
grep -q 'VERSION="1.2.3"' "$tmp/out/install-plugin.sh"
test -x "$tmp/out/install-plugin.sh"
expected="$(cut -d' ' -f1 "$tmp/out/install-plugin.sh.sha256")"
actual="$(openssl dgst -sha256 -r "$tmp/out/install-plugin.sh" | cut -d' ' -f1)"
test "$expected" = "$actual"

bash "$here/render-release-files.sh" 1.2.3 "$tmp/manifest-only" manifest
test -f "$tmp/manifest-only/pact-plugin.json"
test ! -e "$tmp/manifest-only/install-plugin.sh"

if bash "$here/render-release-files.sh" 1.2 "$tmp/bad" all 2>/dev/null; then
  echo "expected a malformed version to be rejected" >&2
  exit 1
fi
echo "release file tests passed"

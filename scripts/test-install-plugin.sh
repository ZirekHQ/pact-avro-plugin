#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
version=9.9.9

case "$(uname -s)" in Linux) os=linux ;; Darwin) os=osx ;; *) echo "skip: host os"; exit 0 ;; esac
case "$(uname -m)" in x86_64) arch=x86_64 ;; arm64|aarch64) arch=aarch64 ;; *) echo "skip: host arch"; exit 0 ;; esac

release="$tmp/release"
mkdir -p "$release"
printf '#!/bin/sh\necho fake-plugin\n' > "$tmp/pact-avro-plugin"
gzip -c "$tmp/pact-avro-plugin" > "$release/pact-avro-plugin-$os-$arch.gz"
(cd "$release" && openssl dgst -sha256 -r "pact-avro-plugin-$os-$arch.gz" > "pact-avro-plugin-$os-$arch.gz.sha256")
bash "$here/render-release-files.sh" "$version" "$release" manifest
bash "$here/render-release-files.sh" "$version" "$tmp/out" all

run_install() {
  PACT_AVRO_PLUGIN_BASE_URL="file://$release" PACT_PLUGIN_DIR="$tmp/plugins" sh "$tmp/out/install-plugin.sh"
}

run_install
dest="$tmp/plugins/avro-$version"
test -x "$dest/pact-avro-plugin"
test "$("$dest/pact-avro-plugin")" = fake-plugin
grep -q "\"version\": \"$version\"" "$dest/pact-plugin.json"

rm -rf "$tmp/plugins"
echo tampered > "$release/pact-avro-plugin-$os-$arch.gz.sha256"
if run_install 2>/dev/null; then
  echo "expected a checksum mismatch to fail the install" >&2
  exit 1
fi
test ! -e "$dest/pact-avro-plugin"
test ! -e "$dest/pact-plugin.json"
echo "install script tests passed"

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
  PATH="${install_path:-$PATH}" PACT_AVRO_PLUGIN_BASE_URL="file://$release" PACT_PLUGIN_DIR="$tmp/plugins" sh "$tmp/out/install-plugin.sh"
}

run_install
dest="$tmp/plugins/avro-$version"
test -x "$dest/pact-avro-plugin"
test "$("$dest/pact-avro-plugin")" = fake-plugin
grep -q "\"version\": \"$version\"" "$dest/pact-plugin.json"

sha_file="$release/pact-avro-plugin-$os-$arch.gz.sha256"

assert_install_rejected() {
  local expected_message="$1"
  rm -rf "$tmp/plugins"
  if run_install 2>"$tmp/stderr"; then
    echo "expected the install to fail: $expected_message" >&2
    exit 1
  fi
  grep -qF "$expected_message" "$tmp/stderr"
  test ! -e "$dest/pact-avro-plugin"
  test ! -e "$dest/pact-plugin.json"
}

echo tampered > "$sha_file"
assert_install_rejected "checksum mismatch"

: > "$sha_file"
assert_install_rejected "empty checksum"

echo "tampered" > "$sha_file"
no_hash_bin="$tmp/no-hash-bin"
mkdir -p "$no_hash_bin"
for tool in sh cut curl gunzip gzip mktemp mkdir chmod rm mv uname cat; do
  ln -s "$(command -v "$tool")" "$no_hash_bin/$tool"
done
install_path="$no_hash_bin" assert_install_rejected "sha256sum or shasum is required"
echo "install script tests passed"

#!/usr/bin/env bash
set -euo pipefail

VERSION=99.9.9
crate=modules/plugin-rs
exe=pact-avro-plugin
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) exe=pact-avro-plugin.exe ;;
  *) ;;
esac
dest="${PACT_PLUGIN_DIR:-$HOME/.pact/plugins}/avro-${VERSION}"

echo '== Installing Rust plugin =='
mkdir -p "$dest"
bash scripts/render-release-files.sh "$VERSION" "$dest" manifest
cp "$crate/target/release/$exe" "$dest/$exe"
chmod +x "$dest/$exe"

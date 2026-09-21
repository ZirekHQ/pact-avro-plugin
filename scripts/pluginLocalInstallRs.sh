#!/bin/bash
set -e

VERSION=99.9.9
BINARY=modules/plugin-rs/target/release/pact-avro-plugin
DEST=~/.pact/plugins/avro-${VERSION}

echo '== Installing Rust plugin =='
mkdir -p "${DEST}"
cp modules/plugin-rs/pact-plugin.json "${DEST}/pact-plugin.json"
cp "${BINARY}" "${DEST}/pact-avro-plugin"
chmod +x "${DEST}/pact-avro-plugin"

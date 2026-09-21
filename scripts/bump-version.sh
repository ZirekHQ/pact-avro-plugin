#!/usr/bin/env bash
set -euo pipefail

new_version="${1:?usage: scripts/bump-version.sh <new-version>}"
if ! [[ "$new_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "::error::'${new_version}' is not an X.Y.Z version" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)/modules/plugin-rs"

old_version="$(awk '/^\[package\]/{f=1;next} /^\[/{f=0} f && /^version = /{gsub(/version = "|"/,""); print; exit}' Cargo.toml)"
if [[ -z "$old_version" ]]; then
  echo "::error::Could not find [package].version in Cargo.toml" >&2
  exit 1
fi

sed -i.bak "0,/^version = \"${old_version}\"\$/s//version = \"${new_version}\"/" Cargo.toml
rm -f Cargo.toml.bak
cargo update --workspace --quiet

echo "Bumped ${old_version} -> ${new_version}"
git diff --stat -- Cargo.toml Cargo.lock

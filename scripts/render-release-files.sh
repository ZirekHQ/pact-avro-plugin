#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: render-release-files.sh <version> <outdir> [manifest|all]}"
outdir="${2:?usage: render-release-files.sh <version> <outdir> [manifest|all]}"
what="${3:-all}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "::error::'${version}' is not an X.Y.Z version" >&2
  exit 1
fi

render() { local src="$1" dest="$2"; sed "s/@VERSION@/${version}/g" "$src" > "$dest"; }

mkdir -p "$outdir"
render "$root/scripts/release/pact-plugin.json.tmpl" "$outdir/pact-plugin.json"

if [[ "$what" == all ]]; then
  render "$root/scripts/release/install-plugin.sh.tmpl" "$outdir/install-plugin.sh"
  chmod +x "$outdir/install-plugin.sh"
  (cd "$outdir" && openssl dgst -sha256 -r install-plugin.sh > install-plugin.sh.sha256)
fi

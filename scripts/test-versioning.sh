#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git init -q -b main "$tmp"
cd "$tmp"
git config user.email test@example.com
git config user.name test
git config commit.gpgsign false
git config tag.gpgsign false
git commit -q --allow-empty -m "feat: initial"

expect() {
  local want_out="$1" want_rc="$2" got rc=0
  got="$(bash "$here/next-version.sh" 2>/dev/null)" || rc=$?
  if [ "$rc" != "$want_rc" ] || [ "$got" != "$want_out" ]; then
    echo "FAIL: expected rc=$want_rc out='$want_out', got rc=$rc out='$got'" >&2
    exit 1
  fi
}

reset() { git tag -f v0.0.6 HEAD >/dev/null; }
commit() { git commit -q --allow-empty -m "$1"${2:+ -m "$2"}; }

rc=0; bash "$here/next-version.sh" >/dev/null 2>&1 || rc=$?
test "$rc" = 2

reset;                          expect "" 1
commit "docs: update readme";   expect "" 1
commit "fix(rust): handle x";   expect "0.0.7" 0
reset; commit "feat(rust): y";  expect "0.1.0" 0
reset; commit "feat(rust)!: z"; expect "1.0.0" 0
reset; commit "fix: w" "BREAKING CHANGE: v"; expect "1.0.0" 0
reset; commit "ci: tweak";      expect "0.0.7" 0
echo "versioning tests passed"

#!/usr/bin/env bash
set -euo pipefail

tag="${1:?usage: index-pr.sh <vX.Y.Z>  (GH_TOKEN: a token that can push to your fork of pact-plugins)}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
upstream=pact-foundation/pact-plugins
version="${tag#v}"
branch="add-avro-${version}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fork_owner="$(gh api user --jq .login)"
if [[ "$(gh pr list --repo "$upstream" --head "${fork_owner}:${branch}" --state open --json number --jq length)" -gt 0 ]]; then
  echo "an open pull request for ${branch} already exists; nothing to do"
  exit 0
fi

bash "$here/fetch-pact-plugin-cli.sh" "$tmp/bin"
export PACT_PLUGIN_CLI="$tmp/bin/pact-plugin-cli"

git clone --quiet --depth 1 "https://github.com/${upstream}.git" "$tmp/repo"
cd "$tmp/repo"
git switch --quiet -c "$branch"

previous_had_jvm=false
if grep -q '^\[\[entries\.avro\.versions\]\]$' repository/repository.index; then
  # The most recently added version's block is the last one in the file;
  # check whether it lists a `jvm` dependency before this run adds a new one.
  if awk '/^\[\[entries\.avro\.versions\]\]$/ { block = "" } { block = block "\n" $0 } END { print block }' \
      repository/repository.index | tail -30 | grep -q 'dependencies = \[.*"jvm"'; then
    previous_had_jvm=true
  fi
fi

status="$(bash "$here/index-add-version.sh" repository/repository.index "$tag")"
if [[ "$status" == present ]]; then
  echo "avro ${version} is already in the index; nothing to do"
  exit 0
fi

user_id="$(gh api user --jq .id)"
git config user.name "$fork_owner"
git config user.email "${user_id}+${fork_owner}@users.noreply.github.com"
git add repository/repository.index repository/repository.index.sha256
git commit --quiet -m "Add avro ${version} version"
git diff --stat HEAD~1

if [[ -n "${INDEX_PR_DRY_RUN:-}" ]]; then
  echo "dry run: not pushing"
  exit 0
fi

gh repo view "${fork_owner}/pact-plugins" >/dev/null 2>&1 || gh repo fork "$upstream" --clone=false
git remote add fork "https://github.com/${fork_owner}/pact-plugins.git"
if git ls-remote --exit-code --heads fork "$branch" >/dev/null 2>&1; then
  git fetch --quiet fork "refs/heads/${branch}:refs/remotes/fork/${branch}"
fi
attempts=0
# The clone is shallow, so the fork must already hold upstream's history or the push is rejected.
until gh repo sync "${fork_owner}/pact-plugins" && \
  git -c credential.helper='!gh auth git-credential' push --quiet --force-with-lease fork "$branch"; do
  attempts=$((attempts + 1))
  [[ "$attempts" -lt 5 ]] || { echo "::error::could not sync the fork and push ${branch}" >&2; exit 1; }
  sleep 5
done

jvm_note=""
if [[ "$previous_had_jvm" == true ]]; then
  jvm_note=$'\n\nThe plugin is now a native Rust executable, so the manifest has no `jvm` dependency and uses per-OS entry points.'
fi

body="Adds avro ${version} to the repository index.${jvm_note}

Release: https://github.com/ZirekHQ/pact-avro-plugin/releases/tag/${tag}

Generated with \`pact-plugin-cli repository add-plugin-version git-hub\` and checked with \`repository validate\`."
gh pr create --repo "$upstream" --base main --head "${fork_owner}:${branch}" \
  --title "Add avro ${version} version" --body "$body"

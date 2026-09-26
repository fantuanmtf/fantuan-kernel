#!/usr/bin/env bash
# Owner-run: create/update the local fantuan-apps and package branches from
# main with the skeletons under build/branch-skeletons. This script does NOT
# push; it prints the commands to publish the branches.
#
#   tools/mkbranches.sh
#   MKBRANCHES_BASE=main MKBRANCHES_FORCE=1 tools/mkbranches.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BASE="${MKBRANCHES_BASE:-main}"
SKEL="$ROOT/build/branch-skeletons"
BRANCHES="fantuan-apps package"

die() { echo "mkbranches: $*" >&2; exit 1; }

git rev-parse --verify --quiet "$BASE^{commit}" >/dev/null || die "base $BASE not found"
[ -z "$(git status --porcelain)" ] || die "working tree is dirty; commit or stash first (build/ is ignored)"
[ -d "$SKEL/fantuan-apps" ] && [ -d "$SKEL/package" ] || die "skeletons missing under build/branch-skeletons"
current="$(git symbolic-ref --short -q HEAD || true)"
for branch in $BRANCHES; do
  [ "$current" = "$branch" ] && die "$branch is checked out; switch to $BASE first"
done

# The package branch always carries the current client.
rm -rf "$SKEL/package/tools/appctl"
mkdir -p "$SKEL/package/tools"
cp -a "$ROOT/tools/appctl" "$SKEL/package/tools/appctl"
find "$SKEL" -name '__pycache__' -type d -exec rm -rf {} + 2>/dev/null || true

build_branch() { # branch skeleton message
  local branch="$1" skeleton="$2" message="$3" index tree commit file rel blob mode
  if git rev-parse --verify --quiet "refs/heads/$branch" >/dev/null; then
    if ! git merge-base --is-ancestor "refs/heads/$branch" "$BASE"; then
      [ "${MKBRANCHES_FORCE:-0}" = "1" ] \
        || die "$branch has local commits not in $BASE; set MKBRANCHES_FORCE=1 to move it"
    fi
  fi
  index="$(mktemp -t mkbranches.XXXXXX)"
  rm -f "$index"
  GIT_INDEX_FILE="$index" git read-tree "$BASE^{tree}"
  while IFS= read -r -d '' file; do
    rel="${file#"$skeleton"/}"
    blob="$(git hash-object -w "$file")"
    if [ -x "$file" ]; then mode=100755; else mode=100644; fi
    GIT_INDEX_FILE="$index" git update-index --add --cacheinfo "$mode,$blob,$rel"
  done < <(find "$skeleton" -type f -print0)
  tree="$(GIT_INDEX_FILE="$index" git write-tree)"
  rm -f "$index"
  commit="$(git commit-tree "$tree" -p "refs/heads/$BASE" -m "$message")"
  git update-ref "refs/heads/$branch" "$commit"
  echo "mkbranches: $branch -> $(git rev-parse --short "$commit") ($message)"
}

build_branch fantuan-apps "$SKEL/fantuan-apps" "catalog: fantuan-apps branch skeleton (C2)"
build_branch package "$SKEL/package" "package: appctl tooling branch skeleton (C2)"

echo
echo "Local branches based on $BASE (not pushed). The skeleton README.md becomes"
echo "the branch landing page; main keeps the kernel README and LICENSE."
echo
echo "Publish (owner only). These branches are *rebuilt* on top of $BASE, so the"
echo "push is a force one; the lease pins the published value each mirror holds"
echo "now, so a concurrent push is refused rather than clobbered."
echo "Codeberg mirrors main only - never push a branch there (docs/REPO_POLICY.md)."
for branch in $BRANCHES; do
  old="$(git rev-parse --verify -q "refs/remotes/origin/$branch" || true)"
  echo "  git push --force-with-lease=$branch:${old:-<expected-sha>} origin $branch"
  echo "  git push --force-with-lease=$branch:${old:-<expected-sha>} gitlab $branch"
done

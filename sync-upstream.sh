#!/usr/bin/env bash
set -euo pipefail
# Sync upstream zed-industries/zed into our public fork
# Usage: ./sync-upstream.sh [--dry-run]

DRY_RUN="${1:-}"
UPSTREAM="origin"       # https://github.com/zed-industries/zed.git
FORK="fork"             # https://github.com/toxicwind/zed.git

# Find our fork commits by looking for commits NOT on upstream/main
echo "=== Syncing $UPSTREAM/main → $FORK/main ==="
git fetch "$UPSTREAM" main

OUR_BASE=$(git merge-base HEAD "$UPSTREAM/main")
UPSTREAM_SHA=$(git rev-parse "$UPSTREAM/main")
HEAD_SHA=$(git rev-parse HEAD)

echo "Upstream SHA: $UPSTREAM_SHA"
echo "Fork HEAD:    $HEAD_SHA"
echo "Merge base:   $OUR_BASE"

if [ "$UPSTREAM_SHA" = "$OUR_BASE" ]; then
    echo "Already up to date with upstream."
elif [ "$HEAD_SHA" = "$UPSTREAM_SHA" ]; then
    echo "ERROR: HEAD matches upstream — our patches are missing!"
    echo "Run: git reset --hard ORIG_HEAD  (if available)"
    exit 1
else
    echo "Rebasing our patches onto latest upstream..."
    # Stash any unstaged changes first
    git stash push -m "sync-upstream-$(date +%s)" 2>/dev/null || true
    if GIT_EDITOR=true git rebase "$UPSTREAM/main"; then
        echo "Rebase succeeded."
    else
        echo "Rebase conflict! Manual resolution needed."
        echo "Resolve conflicts, then: git rebase --continue"
        echo "To abort: git rebase --abort"
        exit 1
    fi
    git stash pop 2>/dev/null || true
fi

echo "Pushing to fork..."
if [ -z "$DRY_RUN" ]; then
    git push --force "$FORK" main
fi
echo "=== Sync complete ==="
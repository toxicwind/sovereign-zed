#!/usr/bin/env bash
set -euo pipefail
# Sync upstream zed-industries/zed into our public fork
# Usage: ./sync-upstream.sh [--dry-run]

DRY_RUN="${1:-}"
UPSTREAM="origin"       # https://github.com/zed-industries/zed.git
FORK="fork"             # https://github.com/toxicwind/zed.git

echo "=== Syncing $UPSTREAM/main → $FORK/main ==="
git fetch "$UPSTREAM" main
git checkout main
UPSTREAM_SHA=$(git rev-parse "$UPSTREAM/main")
OUR_SHA=$(git rev-parse "main")
OUR_CHERRY=$(git log main --oneline --grep="agent: Respect .ignore" --format="%H" | head -n1)

echo "Upstream:  $UPSTREAM_SHA"
echo "Private:   $OUR_SHA"
echo "Our patch: $OUR_CHERRY"

if [ "$UPSTREAM_SHA" = "$OUR_SHA" ]; then
    echo "Already in sync."
    exit 0
fi

# Super-merge: rebase our patch on top of latest upstream
if [ -n "$OUR_CHERRY" ]; then
    echo "Rebasing our patch onto latest upstream..."
    git reset --hard "$UPSTREAM/main"
    GIT_EDITOR=true git cherry-pick "$OUR_CHERRY" || {
        echo "Cherry-pick conflict! Manual resolution needed."
        echo "Files with conflicts:"
        git diff --name-only --diff-filter=U
        exit 1
    }
fi

echo "Pushing to fork..."
if [ -z "$DRY_RUN" ]; then
    git push --force "$FORK" main
fi
echo "=== Sync complete ==="

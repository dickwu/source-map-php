#!/usr/bin/env bash
# Mirror the installed skill at ~/.claude/skills/source-map-php/
# into <repo>/skills/source-map-php/ so it ships with the source-map-php repo
# and can be republished to wherever skills live (skill.sh, etc.).
#
# Usage: sync_to_repo.sh [repo-root]
#   default repo-root = /Users/gwddeveloper/opensource/source-map-php
#
# This script NEVER syncs in the reverse direction — the installed copy is the source of truth.

set -euo pipefail

SRC="$HOME/.claude/skills/source-map-php"
REPO="${1:-$HOME/opensource/source-map-php}"
DEST="$REPO/skills/source-map-php"

if [ ! -d "$SRC" ]; then
  echo "source not found: $SRC" >&2
  exit 1
fi
if [ ! -d "$REPO" ]; then
  echo "repo root not found: $REPO" >&2
  exit 1
fi

mkdir -p "$DEST"

# Use rsync when available — deletes stale files in DEST but only under skills/source-map-php/.
if command -v rsync >/dev/null 2>&1; then
  rsync -a --delete --exclude='.DS_Store' "$SRC/" "$DEST/"
else
  rm -rf "$DEST"
  cp -R "$SRC" "$DEST"
fi

echo "synced $SRC -> $DEST"
echo
echo "If you plan to publish to skill.sh (or any other skill registry),"
echo "commit this directory and push. The repo copy is the publishable artifact."

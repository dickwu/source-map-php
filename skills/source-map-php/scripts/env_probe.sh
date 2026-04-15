#!/usr/bin/env bash
# Health check for source-map-php.
# Prints: binary version, Meili source, Meili reachability.
# Never prints the master key itself — only confirms it is set.

set -u

fail=0

# Binary check
if ! command -v source-map-php >/dev/null 2>&1; then
  printf 'source-map-php: NOT INSTALLED\n  install: cargo install source-map-php  OR  brew install dickwu/tap/source-map-php\n'
  exit 1
fi
ver="$(source-map-php --version 2>/dev/null || echo 'unknown')"
printf 'source-map-php: %s (%s)\n' "$ver" "$(command -v source-map-php)"

# Meili host/key resolution
connect_json="$HOME/.config/meilisearch/connect.json"
host=""
key_present=0
source=""

if [ -n "${MEILI_HOST:-}" ]; then
  host="$MEILI_HOST"
  source="env"
  [ -n "${MEILI_MASTER_KEY:-}" ] && key_present=1
elif [ -f "$connect_json" ]; then
  host="$(python3 -c "import json,sys; d=json.load(open('$connect_json')); print(d.get('host',''))" 2>/dev/null)"
  k="$(python3 -c "import json,sys; d=json.load(open('$connect_json')); print(d.get('master_key',''))" 2>/dev/null)"
  source="connect.json"
  [ -n "$k" ] && key_present=1
fi

if [ -z "$host" ]; then
  printf 'meili host: NOT CONFIGURED\n  set MEILI_HOST or create %s\n' "$connect_json"
  fail=1
else
  printf 'meili host: %s (from %s)\n' "$host" "$source"
  if [ "$key_present" -eq 1 ]; then
    printf 'meili key : present (from %s)\n' "$source"
  else
    printf 'meili key : MISSING (from %s)\n' "$source"
    fail=1
  fi

  # Reachability
  if command -v curl >/dev/null 2>&1; then
    if curl -fsS --max-time 3 "$host/health" >/dev/null 2>&1; then
      printf 'meili reach: OK (%s/health)\n' "$host"
    else
      printf 'meili reach: FAILED (%s/health)\n' "$host"
      fail=1
    fi
  fi
fi

# Project registry
proj_json="$HOME/.config/meilisearch/project.json"
if [ -f "$proj_json" ]; then
  count="$(python3 -c "import json; d=json.load(open('$proj_json')); print(len(d) if isinstance(d, dict) else len(d.get('projects', [])))" 2>/dev/null || echo '?')"
  printf 'projects  : %s saved (%s)\n' "$count" "$proj_json"
else
  printf 'projects  : none saved (no %s yet)\n' "$proj_json"
fi

exit "$fail"

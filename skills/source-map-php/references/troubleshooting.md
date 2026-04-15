# source-map-php troubleshooting

Read this when pre-flight doesn't explain what went wrong.

## Connection errors to Meilisearch

### Symptom

```
error: connection refused / timeout / unauthorized
```

### Diagnose

1. Is the Meili daemon running? On macOS with Homebrew: `brew services list | grep meilisearch`. On Linux/Docker: `docker ps | grep meili`.
2. Does `curl -fsS "$MEILI_HOST/health"` return `{"status":"available"}`?
3. Is the master key correct? The CLI resolves keys in this order:
   - `MEILI_MASTER_KEY` env var
   - `~/.config/meilisearch/connect.json` `"master_key"`
   - (no fallback for the key — the project config only provides a URL default)

### Fix

- If daemon is down: start it (`brew services start meilisearch`, `docker start <container>`, etc.).
- If host is wrong: set `MEILI_HOST` explicitly so env beats the file.
- If key mismatch: inspect `~/.config/meilisearch/connect.json` and compare against the daemon's configured key.

Never paste the master key into chat or logs — confirm agreement by hashing (`shasum`) or by re-running a command that would succeed.

## Framework auto-detect picked the wrong one

### Symptom

Index finishes but routes are empty, or routes look wrong (Laravel routes in a Hyperf project, etc.).

### Fix

Rerun with explicit framework:

```bash
source-map-php index --repo <path> --project-name <name> --framework laravel --mode clean
# or
source-map-php index --repo <path> --project-name <name> --framework hyperf --mode clean
```

Laravel auto-detect looks for `artisan`. Hyperf looks for `config/config.php` + `hyperf` dependencies. Monorepos or non-standard layouts can confuse both.

## Phpactor missing

### Symptom

```
doctor: WARN phpactor not on PATH — falling back to embedded parser
```

### Fix

Optional. The extractor has fallback parsing, so indexing still works — coverage is just somewhat lower for complex type annotations and traits.

If the user wants full coverage:

```bash
brew install phpactor/tap/phpactor   # macOS
# or install from https://phpactor.readthedocs.io/
```

## Indexer crashes on non-UTF-8 files

### Symptom

Traceback / panic partway through scanning a large repo, especially ones with legacy encoding in comments or fixture files.

### Status

Known roadmap item (README "Known limitations"). No clean fix inside the CLI today.

### Workaround

1. Identify offenders: `rg -l --binary '[^\x00-\x7F]' <repo>` or `file -bi <file> | grep -v 'utf-8'`.
2. Temporarily exclude them by tightening the allowlist in `config/indexer.toml`, or convert them to UTF-8 if they're small and you control them.
3. Report encoding examples to the tool's issue tracker so the fix can generalize.

## Search returns nothing unexpectedly

### Diagnose

1. Is the project actually indexed? `source-map-php search --project <name> --query 'the'` — if even a generic term returns nothing, the index is empty or the project name is wrong.
2. Is `~/.config/meilisearch/project.json` pointing at the repo you think? Cat the file.
3. Did the last `index` run actually finish? Check `build/index-runs/` for the latest manifest and look for an error.

### Fix

- Wrong project name: rerun `search --project <exact-name>`.
- Empty index: rerun `index --mode clean`.
- Partial index from a crashed run: rerun with `--mode clean` to rebuild.

## `validate` returns no commands

### Diagnose

The symbol probably doesn't exist or wasn't indexed. Check:

1. Double-backslash in the shell arg (`'App\\Services\\Foo'`).
2. Fully qualified name (with namespace).
3. The project is indexed (see "Search returns nothing" above).

## `verify` shows unexpected changes

### Diagnose

`verify` compares the staged index against live. If it reports many diffs after a small code change:

1. Clean-mode rebuilds always look like a full replacement — stay in `clean` if the user isn't intentionally doing staged rollouts.
2. Config drift (new allowlist/denylist entries) also produces large diffs. Check `config/indexer.toml` for uncommitted changes.

### Fix

If the diff looks right: `promote --run-id <id>`. If not: fix the config or code, re-run `index --mode staged`, re-`verify`.

## When to escalate

Collect:

- `source-map-php --version`
- The exact command and arguments that failed
- The relevant lines of `build/index-runs/<run_id>.json`
- OS, Meili version (`curl "$MEILI_HOST/version"`)

Then point the user at the project's issue tracker with this context. Don't speculate on root cause without evidence.

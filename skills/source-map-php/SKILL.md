---
name: source-map-php
description: Drive the `source-map-php` Rust CLI to build, search, and manage Meilisearch-backed code indexes for Laravel and Hyperf PHP repositories. Invoke this whenever the user wants to index or reindex a PHP project, search indexed code (symbols, routes, tests, packages, schema), ask which tests cover a PHP symbol, run `source-map-php` subcommands (`init`, `doctor`, `index`, `search`, `validate`, `verify`, `promote`, `remove`), set up LLM-friendly code search for a PHP app, or troubleshoot Meilisearch-backed PHP code search — including casual phrasings like "index my laravel repo", "build code search for my hyperf service", "find routes for POST /foo in project X", "what tests cover App\\Services\\Foo::bar", "rebuild the php index after I added a class", or any mention of the `source-map-php` binary, Meilisearch + PHP code search, or the saved project registry at `~/.config/meilisearch/project.json`. Prefer this skill over ad-hoc `grep`/`rg` when the user wants structured, framework-aware search against an existing or new source-map-php index.
---

# source-map-php

`source-map-php` is a Rust CLI that extracts symbols, routes, tests, Composer ownership, and schema hints from Laravel and Hyperf PHP repositories and stores them in Meilisearch for LLM-friendly code search.

This skill helps operate the CLI on behalf of the user. It is not a replacement for general PHP help — invoke it when the user wants to **work with the index**: build it, search it, validate a symbol, clean it up, or diagnose why it's misbehaving.

## When to reach for this skill

- The user names a PHP repo and asks to index, reindex, or search it.
- The user mentions `source-map-php`, Meilisearch + PHP, or LLM-friendly code search.
- The user wants the tests or routes related to a specific PHP symbol.
- The user hits an error from the binary and wants help.

If the user only wants to *understand* PHP code without the index, answer directly — don't reach for the indexer.

## Pre-flight (once per session, then cache)

Before running indexing or search commands:

1. **Binary present and version.** Run `source-map-php --version`. If missing, tell the user how to install (`cargo install source-map-php`, or `brew install dickwu/tap/source-map-php`) and stop.
2. **Meilisearch reachable.** The CLI reads `MEILI_HOST` + `MEILI_MASTER_KEY` from env, then falls back to `~/.config/meilisearch/connect.json`, then the project config default. Check the values resolve — do not print secrets.
3. **Project config.** `source-map-php init` writes `config/indexer.toml` plus a placeholder `~/.config/meilisearch/connect.json`. First-time users in a fresh directory need this. `init` will not overwrite an existing connect.json.

For a thorough environment check, run `source-map-php doctor --repo <path>` and relay what it reports. `doctor` still expects Phpactor on `PATH`; the extractor has fallback parsing if Phpactor is missing, so mention it's a warning not a blocker.

There is a helper at `scripts/env_probe.sh` that runs these checks in one shot — use it when the user explicitly wants a health probe.

## The six workflows

### 1. First-time setup

```bash
source-map-php init --dir .
cp .env.example .env     # user fills MEILI_HOST and MEILI_MASTER_KEY
source-map-php doctor --repo /absolute/path/to/php-repo
```

Ask the user to fill the `.env` before moving on. Don't guess the Meili host — if it isn't already configured, pause and ask.

### 2. Build or rebuild an index

```bash
source-map-php index \
  --repo /absolute/path/to/php-repo \
  --project-name <short-name> \
  --framework auto \
  --mode clean
```

- `--mode clean` is the default and what you want almost always — full rebuild.
- `--mode staged` is for a safe rollout where the user wants to `verify` before `promote`. Don't reach for it on routine reindex.
- `--framework auto` works for standard layouts. Only override if the user explicitly names Laravel or Hyperf, or auto-detection was wrong.
- `--project-name` is the handle the user will pass to `search --project` later. Default it to the repo's directory name if the user doesn't supply one.

After indexing, surface the run id from the output (also written to `build/index-runs/<run_id>.json`) — needed for `promote` in staged mode.

### 3. Search the index

```bash
source-map-php search --project <name-or-path> --query "<natural query>"
```

- `--project` accepts either the saved project name or the absolute repo path.
- Omit `--index` for the default `all` — grouped results across symbols, routes, tests, packages, and schema. Narrow it only when the user's intent is clearly scoped (see the slice guide below).
- Add `--json` when another tool will consume the output (piping into jq, feeding back into the conversation, etc.).
- Show the CLI's output verbatim instead of paraphrasing — the user wants to see where matches live.

### 4. Validation commands for a symbol

```bash
source-map-php validate --symbol 'App\\Services\\ConsentService::sign'
```

Double-escape backslashes in shell arguments. Add `--json` for structured output. The CLI returns ranked test/validation commands. Surface the list — only execute the commands if the user asks you to.

### 5. Staged → verify → promote

Reach for this only when the user explicitly wants a safe rollout:

```bash
source-map-php index --mode staged --repo <path> --project-name <name>
source-map-php verify
source-map-php promote --run-id <id>
```

If the user just wants to search, `--mode clean` is correct — skip staged.

### 6. Remove a project

```bash
source-map-php remove --project <name>
```

Add `--keep-indexes` when the user wants to forget the saved name but keep the Meili indexes intact (useful during a project rename). Confirm before removing — the Meili side is not recoverable from the CLI.

## Choosing an `--index` slice

| User intent | `--index` |
|---|---|
| "where is the X class defined", "what service does Y" | `symbols` (or leave as `all`) |
| "what route serves POST /foo" | `routes` |
| "which tests cover Z" | `tests` |
| "what package owns this file" | `packages` |
| "show me the patients table schema" | `schema` |
| Anything ambiguous | `all` (default) |

Grouped `all` results are usually more informative than guessing a slice.

## Sanity-first error recovery

1. **Empty results, no error.** The query just didn't match — suggest broader terms or a different `--index`.
2. **Meilisearch connection refused.** Daemon isn't up, or env points to the wrong host. Show the resolution order (env → `~/.config/meilisearch/connect.json` → project default) and ask which the user expects to win.
3. **`doctor` warns about Phpactor.** Fallback parsing still works; optionally suggest installing Phpactor for better symbol coverage.
4. **Auto-detect picks the wrong framework.** Rerun with explicit `--framework laravel` or `--framework hyperf`.
5. **Non-UTF-8 files in the tree crash the indexer.** Known limitation (README roadmap item) — call it out honestly, don't thrash trying to fix it.

When a command errors, read the Rust CLI's message carefully before speculating — it usually prints the actionable thing.

See `references/troubleshooting.md` for deeper diagnosis steps.

## Safety model (context, not a lecture)

The indexer has a built-in sanitizer that drops strings matching patterns for API keys, JWTs, private keys, passwords/DSNs, emails, phone numbers, long numeric IDs, DOB-like strings, and medical-record-style identifiers *before* they reach Meilisearch. That's by design — this tool is for source-code metadata, not application data.

Don't reiterate this on every command. If the user asks whether it's safe to index repo X, the honest answer is: yes for source code, and the sanitizer catches most accidental secrets, but avoid pointing it at repos whose fixtures contain real PII.

## Output style for results

- Print the exact command you ran on one line so the user can copy it.
- For `search` and `validate`, show the CLI output verbatim.
- For `doctor`, `verify`, `promote`, summarize — those outputs are long and the user just wants the verdict.

## Multi-project workflows

`~/.config/meilisearch/project.json` is the registry of saved projects. When the user juggles several PHP repos:

- Give each project a distinct name (`patient-api`, `billing-svc`, `admin-panel`).
- Scope searches with `--project <name>`.
- Only search across everything when the user clearly wants that.

## What this skill does NOT do

- Edit PHP code inside the target repo.
- Run PHPUnit / Pest suites — `validate` surfaces the commands; the user or a separate step runs them.
- Manage the Meilisearch daemon. If Meili isn't running, tell the user to start it and stop.

## Reference files

- `references/commands.md` — full flag reference for every subcommand. Read this when you need exact syntax you can't remember.
- `references/troubleshooting.md` — deeper diagnosis steps for failure modes that pre-flight doesn't catch.
- `scripts/env_probe.sh` — health check for the binary and Meili connectivity.
- `scripts/sync_to_repo.sh` — keep the in-repo copy under `<repo>/skills/source-map-php/` in sync with the installed version at `~/.claude/skills/source-map-php/`.

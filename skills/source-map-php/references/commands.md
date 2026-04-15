# source-map-php command reference

Full flag list for each subcommand. Use this when you need exact syntax.

## init

Scaffold config for a new usage of the tool.

```bash
source-map-php init [--dir <DIR>] [--force]
```

- `--dir <DIR>` — directory to scaffold into. Defaults to current.
- `--force` — overwrite existing files in the target directory (does **not** apply to `~/.config/meilisearch/connect.json`, which is never clobbered).

Writes `config/indexer.toml`, `.env.example`, and a placeholder `~/.config/meilisearch/connect.json` if one doesn't already exist.

## doctor

Environment health check, optionally against a specific repo.

```bash
source-map-php doctor [--repo <REPO>] [--config <CONFIG>]
```

- `--repo <REPO>` — target repo path; without this, only general env checks run.
- `--config <CONFIG>` — custom path to `indexer.toml` (default `config/indexer.toml`).

Expect warnings about Phpactor if it's not on `PATH`. The extractor has fallback parsing, so Phpactor is recommended, not required.

## index

Build or rebuild a Meilisearch index for the repo.

```bash
source-map-php index --repo <REPO> \
  [--project-name <NAME>] \
  [--framework auto|laravel|hyperf] \
  [--mode clean|staged] \
  [--config <CONFIG>]
```

- `--repo <REPO>` — **required** absolute path.
- `--project-name <NAME>` — short handle for `search --project` later; defaults to the repo directory basename.
- `--framework` — `auto` (default) probes for Laravel/Hyperf markers. Force when auto is wrong.
- `--mode` — `clean` (default, full rebuild) or `staged` (produces a parallel index that must be `verify`d and `promote`d).
- `--config <CONFIG>` — custom `indexer.toml` path.

Emits a run manifest at `build/index-runs/<run_id>.json`. Save the run id if the user picked staged mode.

## search

Query the index(es).

```bash
source-map-php search --query <QUERY> \
  [--project <NAME_OR_PATH>] \
  [--index all|symbols|routes|tests|packages|schema] \
  [--framework auto|laravel|hyperf] \
  [--config <CONFIG>] \
  [--json]
```

- `--query <QUERY>` — **required** search string.
- `--project <NAME_OR_PATH>` — saved project name or the absolute repo path. Omit to search across all saved projects.
- `--index` — which slice to hit. Default `all` gives grouped results.
- `--framework` — usually unnecessary; rely on the saved project metadata.
- `--json` — machine-readable output.

## validate

Return ranked validation/test commands for a specific PHP symbol.

```bash
source-map-php validate --symbol <SYMBOL> [--config <CONFIG>] [--json]
```

- `--symbol <SYMBOL>` — fully qualified name, double-escape backslashes in shell (e.g. `'App\\Services\\Foo::bar'`).

Output is a ranked list of commands to run. Surface them — only execute on user request.

## verify

Verify what a staged index would change.

```bash
source-map-php verify [--config <CONFIG>]
```

Run between `index --mode staged` and `promote`.

## promote

Swap a staged index into the live slot.

```bash
source-map-php promote [--config <CONFIG>] [--run-id <RUN_ID>]
```

- `--run-id <RUN_ID>` — run id from the prior staged index; if omitted, the CLI tries the latest pending.

## remove

Delete a saved project, optionally preserving its Meili indexes.

```bash
source-map-php remove --project <NAME_OR_PATH> [--keep-indexes] [--config <CONFIG>]
```

- `--keep-indexes` — forget the saved name but keep the Meili indexes intact (useful during rename).

Destructive by default — confirm with the user before removing a project whose indexes can't be cheaply rebuilt.

## Environment variables

| Var | Purpose |
|---|---|
| `MEILI_HOST` | Meilisearch URL, e.g. `http://127.0.0.1:7700`. |
| `MEILI_MASTER_KEY` | Master key for the Meili instance. |

Resolution order (first wins):
1. Env (`MEILI_HOST` + `MEILI_MASTER_KEY`)
2. `~/.config/meilisearch/connect.json`
3. Project config default for URL only (no key)

## Files the CLI touches

| Path | Purpose |
|---|---|
| `config/indexer.toml` | Project-level config scaffolded by `init`. |
| `.env` / `.env.example` | Meili credentials the user fills. |
| `~/.config/meilisearch/connect.json` | Shared Meili credentials across projects. |
| `~/.config/meilisearch/project.json` | Registry of saved project names ↔ repo paths. |
| `build/index-runs/<run_id>.json` | Per-run manifest written on each `index` run. |

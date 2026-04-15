# source-map-php

`source-map-php` is a Rust CLI for building an LLM-friendly code index from Laravel and Hyperf repositories.

It scans PHP project code, extracts symbols and framework metadata, sanitizes sensitive strings before indexing, stores the result in Meilisearch, and gives you search plus validation commands from the command line.

## What it does

- Indexes PHP project code into Meilisearch
- Detects Laravel and Hyperf repositories
- Extracts symbols, comments, package ownership, tests, and schema metadata
- Links symbols to related tests and emits validation commands
- Supports clean and staged indexing flows
- Keeps the product CLI-first. No always-on app server required

## Current status

This is a working baseline, not a finished platform.

What is already in place:

- CLI commands for `init`, `doctor`, `index`, `search`, `validate`, `verify`, and `promote`
- Phpactor-backed symbol extraction with fallback parsing
- Laravel route extraction
- Hyperf route extraction
- Test linking and validation command ranking
- Fixture coverage for one Laravel repo and one Hyperf repo
- CI for formatting, linting, tests, and tagged release builds

What is still being hardened:

- Real-world Hyperf route extraction edge cases
- Non-UTF-8 junk files inside scanned trees
- End-to-end indexing against a live Meilisearch instance in CI

## Why this exists

Generic grep is too noisy for code-aware LLM workflows, and model indexing approaches that serialize application data are the wrong fit for safety-sensitive teams.

`source-map-php` is built for source-code metadata only:

- symbols
- routes
- tests
- package ownership
- schema hints
- comments and PHPDoc

It is explicitly not a production data search engine.

## Requirements

Build requirements:

- Rust toolchain
- PHP 8.3+
- Composer
- Meilisearch

Recommended for best extraction quality:

- Phpactor on `PATH`

The current CI builds and tests on Linux and macOS.

## Install

Until GitHub release artifacts are published, build from source:

```bash
git clone git@github.com:dickwu/source-map-php.git
cd source-map-php
cargo build --release
```

The binary will be at:

```bash
./target/release/source-map-php
```

## Quick start

### 1. Scaffold config

```bash
source-map-php init --dir .
cp .env.example .env
```

`init` also creates `~/.config/meilisearch/connect.json` with placeholder values if that file does not already exist.

Set your Meilisearch connection in `.env`:

```bash
MEILI_HOST=http://127.0.0.1:7700
MEILI_MASTER_KEY=change-me
```

The CLI reads Meilisearch credentials in this order:

1. `MEILI_HOST` and `MEILI_MASTER_KEY` from the environment
2. `~/.config/meilisearch/connect.json`
3. The project config host default for the URL only

`init` will not overwrite an existing `~/.config/meilisearch/connect.json`.

### 2. Check your environment

```bash
source-map-php doctor --repo /path/to/php-repo
```

### 3. Build an index

```bash
source-map-php index --repo /path/to/php-repo --project-name staff-api --framework auto --mode clean
```

### 4. Search the index

```bash
source-map-php search --project staff-api --query "patient consent store"
```

If you omit `--index`, the CLI now searches across all saved index types and prints grouped results.

You can point `--project` at either:

- the saved project name, for example `staff-api`
- the full repository path, for example `/Users/you/work/staff-api`

### 5. Remove a saved project

```bash
source-map-php remove --project staff-api
```

That removes the project entry from `~/.config/meilisearch/project.json` and deletes the matching Meilisearch indexes for that project prefix.

If you only want to forget the saved project name and keep the indexes:

```bash
source-map-php remove --project staff-api --keep-indexes
```

### 6. Ask for validation commands

```bash
source-map-php validate --symbol "App\\Services\\ConsentService::sign"
```

### 7. Verify staged indexes

```bash
source-map-php verify
```

### 8. Promote staged indexes

```bash
source-map-php promote --run-id <run-id>
```

## CLI reference

```bash
source-map-php init [--dir <DIR>] [--force]
source-map-php doctor [--repo <REPO>] [--config <CONFIG>]
source-map-php index --repo <REPO> [--project-name <NAME>] [--framework auto|laravel|hyperf] [--mode clean|staged] [--config <CONFIG>]
source-map-php search --query <QUERY> [--project <NAME_OR_PATH>] [--index all|symbols|routes|tests|packages|schema] [--framework auto|laravel|hyperf] [--config <CONFIG>] [--json]
source-map-php remove --project <NAME_OR_PATH> [--keep-indexes] [--config <CONFIG>]
source-map-php validate --symbol <SYMBOL> [--config <CONFIG>] [--json]
source-map-php verify [--config <CONFIG>]
source-map-php promote [--config <CONFIG>] [--run-id <RUN_ID>]
```

## How indexing works

1. Scan allowlisted project paths
2. Export Composer package ownership metadata
3. Extract symbols and docs from PHP files
4. Enrich with framework-specific route and schema metadata
5. Link tests and generate validation commands
6. Apply Meilisearch settings and write documents
7. Save project metadata to `~/.config/meilisearch/project.json`
8. Emit a run manifest to `build/index-runs/<run_id>.json`

## Default indexing scope

Default allowlist:

- `app/`
- `src/`
- `routes/`
- `config/`
- `database/migrations/`
- `database/factories/`
- `database/seeders/`
- `tests/`
- `test/`
- `composer.json`
- `composer.lock`
- `phpunit.xml`
- `pest.php`

Default deny patterns include:

- `.env*`
- `storage/`
- `bootstrap/cache/`
- `logs/`
- `tmp/`
- database dumps
- spreadsheets
- archives
- `node_modules/`

Vendor indexing is enabled by default, but only for allowlisted vendor paths.

## Safety model

The tool is designed around source-code metadata, not application data.

Before strings are indexed, the sanitizer drops values that look like:

- API keys
- JWTs
- private keys
- passwords and DSNs
- email addresses
- phone numbers
- long numeric IDs
- date-of-birth-like strings
- medical-record-style identifiers

That is the whole game. If it smells like secrets or PHI, it should not reach Meilisearch.

## Output model

The symbol index is designed to support answers like:

- what matched
- where it lives
- why it matched
- related routes
- related tests
- validation commands
- missing-test warning when confidence is weak

## Release flow

CI runs on every push and pull request:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`

Tagged pushes matching `v*` build release artifacts for:

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`

## Development

Common commands:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The repo includes fixture integration tests for:

- Laravel symbol, route, schema, and test extraction
- Hyperf route and test extraction

## Known limitations

- Hyperf extraction currently needs more hardening for large repos with mixed file encodings and route organization styles
- `doctor` currently expects Phpactor to be installed, even though the extractor has fallback behavior
- Release artifacts are wired in CI, but no published versioning policy is documented yet

## Roadmap

- Harden Hyperf route extraction for `config/routers/*.php` and grouped route trees
- Improve fallback parsing for multiline method signatures
- Add live Meilisearch integration coverage
- Publish first tagged binary release

## License

MIT

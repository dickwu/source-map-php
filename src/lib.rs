pub mod adapters;
pub mod composer;
pub mod config;
pub mod extract;
pub mod meili;
pub mod models;
pub mod projects;
pub mod query;
pub mod sanitizer;
pub mod scanner;
pub mod tests_linker;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{IndexerConfig, default_connect_file_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Framework {
    Auto,
    Laravel,
    Hyperf,
}

impl Framework {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Laravel => "laravel",
            Self::Hyperf => "hyperf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexMode {
    Clean,
    Staged,
}

impl IndexMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Staged => "staged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SearchIndex {
    All,
    Symbols,
    Routes,
    Tests,
    Packages,
    Schema,
}

impl SearchIndex {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Symbols => "symbols",
            Self::Routes => "routes",
            Self::Tests => "tests",
            Self::Packages => "packages",
            Self::Schema => "schema",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "source-map-php",
    version,
    about = "CLI-first PHP code search indexer"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        force: bool,
    },
    Doctor {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
    },
    Index {
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        project_name: Option<String>,
        #[arg(long, value_enum, default_value_t = Framework::Auto)]
        framework: Framework,
        #[arg(long, value_enum, default_value_t = IndexMode::Clean)]
        mode: IndexMode,
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
    },
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, value_enum, default_value_t = SearchIndex::All)]
        index: SearchIndex,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        framework: Option<Framework>,
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Remove {
        #[arg(long)]
        project: String,
        #[arg(long)]
        keep_indexes: bool,
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
    },
    Validate {
        #[arg(long)]
        symbol: String,
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Verify {
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
    },
    Promote {
        #[arg(long, default_value = "config/indexer.toml")]
        config: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { dir, force } => init_workspace(&dir, force),
        Commands::Doctor { repo, config } => commands::doctor(&repo, &config),
        Commands::Index {
            repo,
            project_name,
            framework,
            mode,
            config,
        } => commands::index(&repo, project_name.as_deref(), framework, mode, &config),
        Commands::Search {
            query,
            index,
            project,
            framework,
            config,
            json,
        } => commands::search(&query, project.as_deref(), index, framework, &config, json),
        Commands::Remove {
            project,
            keep_indexes,
            config,
        } => commands::remove(&project, keep_indexes, &config),
        Commands::Validate {
            symbol,
            config,
            json,
        } => commands::validate(&symbol, &config, json),
        Commands::Verify { config } => commands::verify(&config),
        Commands::Promote { config, run_id } => commands::promote(&config, run_id.as_deref()),
    }
}

fn init_workspace(dir: &Path, force: bool) -> Result<()> {
    init_workspace_with_connect_path(dir, &default_connect_file_path(), force)
}

fn init_workspace_with_connect_path(dir: &Path, connect_path: &Path, force: bool) -> Result<()> {
    let config_dir = dir.join("config");
    fs::create_dir_all(&config_dir).with_context(|| format!("create {}", config_dir.display()))?;

    write_scaffold(
        &config_dir.join("indexer.toml"),
        &IndexerConfig::default().to_toml_string()?,
        force,
    )?;
    write_scaffold(&dir.join(".env.example"), assets::env_example(), force)?;
    write_scaffold(
        &dir.join("docker-compose.meilisearch.yml"),
        assets::docker_compose_example(),
        force,
    )?;
    let global_template_created =
        write_scaffold_if_missing(connect_path, assets::meili_connect_template())?;

    println!(
        "Initialized source-map-php config in {} at {}",
        dir.display(),
        Utc::now().to_rfc3339()
    );
    if global_template_created {
        println!(
            "Created Meilisearch connect template at {}",
            connect_path.display()
        );
    } else {
        println!(
            "Left existing Meilisearch connect file unchanged at {}",
            connect_path.display()
        );
    }
    Ok(())
}

fn write_scaffold(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists, rerun with --force to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory for {}", path.display()))?;
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_scaffold_if_missing(path: &Path, content: &str) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory for {}", path.display()))?;
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))?;
    Ok(true)
}

mod assets {
    pub fn env_example() -> &'static str {
        "MEILI_HOST=http://127.0.0.1:7700\nMEILI_MASTER_KEY=change-me\n"
    }

    pub fn docker_compose_example() -> &'static str {
        "services:\n  meilisearch:\n    image: getmeili/meilisearch:v1.12\n    ports:\n      - \"7700:7700\"\n    environment:\n      MEILI_ENV: production\n      MEILI_MASTER_KEY: \"${MEILI_MASTER_KEY}\"\n      MEILI_NO_ANALYTICS: \"true\"\n    volumes:\n      - meili_data:/meili_data\n    restart: unless-stopped\n\nvolumes:\n  meili_data:\n"
    }

    pub fn meili_connect_template() -> &'static str {
        "{\n  \"url\": \"http://127.0.0.1:7700\",\n  \"apiKey\": \"change-me\"\n}\n"
    }
}

pub mod commands {
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use anyhow::{Context, Result, anyhow, bail};
    use chrono::Utc;
    use serde_json::json;
    use sha1::{Digest, Sha1};

    use crate::adapters;
    use crate::composer::{ComposerExport, export_packages};
    use crate::config::IndexerConfig;
    use crate::extract::extract_symbols;
    use crate::meili::{
        MeiliClient, packages_settings, routes_settings, runs_settings, schema_settings,
        symbols_settings, tests_settings,
    };
    use crate::models::{
        PackageDoc, RouteDoc, RunManifest, SchemaDoc, SymbolDoc, TestDoc, make_stable_id,
        manifest_path, run_id,
    };
    use crate::projects::{ProjectRecord, ProjectRegistry, default_project_registry_path};
    use crate::query::compact_query;
    use crate::sanitizer::Sanitizer;
    use crate::scanner::scan_repo;
    use crate::tests_linker::{extract_tests, link_symbols_and_routes};
    use crate::{Framework, IndexMode, SearchIndex};

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct SymbolSearchDoc {
        fqn: String,
        path: String,
        line_start: usize,
        package_name: String,
        #[serde(default)]
        related_tests: Vec<String>,
        #[serde(default)]
        missing_test_warning: Option<String>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RouteSearchDoc {
        method: String,
        uri: String,
        action: Option<String>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct TestSearchDoc {
        fqn: String,
        command: String,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct PackageSearchDoc {
        name: String,
        version: Option<String>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct SchemaSearchDoc {
        operation: String,
        table: Option<String>,
        path: String,
        line_start: usize,
    }

    pub fn doctor(repo: &Path, config: &Path) -> Result<()> {
        let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
        load_env_for(config);
        let config = IndexerConfig::load(config)?;

        let checks = vec![
            ("php", command_exists("php"), true),
            ("composer", command_exists("composer"), true),
            ("phpactor", command_exists("phpactor"), false),
            ("git", command_exists("git"), true),
        ];
        for (name, ok, _) in &checks {
            println!("{name:10} {}", if *ok { "ok" } else { "missing" });
        }

        let packages = export_packages(&repo).ok();
        let framework = packages
            .as_ref()
            .map(|packages| {
                adapters::detect_framework(
                    &repo,
                    Framework::Auto,
                    &packages
                        .packages
                        .iter()
                        .map(|package| package.name.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or(Framework::Auto);
        println!("framework  {}", framework.as_str());

        match config.resolve_meili() {
            Ok(connection) => {
                let client = MeiliClient::new(connection)?;
                let health = client.health()?;
                println!("meilisearch ok {health}");
            }
            Err(err) => {
                println!("meilisearch missing {err}");
            }
        }

        if framework == Framework::Laravel {
            let ok = Command::new("php")
                .arg("artisan")
                .arg("--version")
                .current_dir(&repo)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            println!("laravel-artisan {}", if ok { "ok" } else { "missing" });
        }
        if framework == Framework::Hyperf {
            let ok = Command::new("php")
                .arg("bin/hyperf.php")
                .arg("--help")
                .current_dir(&repo)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            println!("hyperf-cli {}", if ok { "ok" } else { "missing" });
        }

        if checks.iter().any(|(_, ok, required)| *required && !ok) {
            bail!("doctor found missing required dependencies");
        }
        Ok(())
    }

    pub fn index(
        repo: &Path,
        project_name: Option<&str>,
        requested_framework: Framework,
        mode: IndexMode,
        config_path: &Path,
    ) -> Result<()> {
        let repo = repo
            .canonicalize()
            .with_context(|| format!("open {}", repo.display()))?;
        load_env_for(config_path);
        let config = IndexerConfig::load(config_path)?;
        let sanitizer = Sanitizer::default();

        let packages = export_packages(&repo)?;
        let package_names = packages
            .packages
            .iter()
            .map(|package| package.name.clone())
            .collect::<Vec<_>>();
        let framework = adapters::detect_framework(&repo, requested_framework, &package_names);
        let repo_name = packages.root.name.clone();
        let files = scan_repo(&repo, &config.paths)?;

        let mut symbols =
            extract_symbols(&repo, &repo_name, framework, &files, &packages, &sanitizer)?;
        let mut routes = adapters::extract_routes(&repo, &repo_name, framework, &sanitizer)?;
        let schema = adapters::extract_schema(&repo, &repo_name)?;
        let mut tests = if config.tests.include_tests {
            extract_tests(&repo, &repo_name, framework, &files)?
        } else {
            Vec::new()
        };
        link_symbols_and_routes(&mut symbols, &mut routes, &mut tests, &config.tests);
        link_routes_to_symbols(&mut symbols, &routes);
        let packages_docs = package_docs(&repo_name, &packages);

        let prefix = config.effective_index_prefix(&repo);
        let run_id = run_id(&repo.display().to_string(), framework, mode);
        let indexes = build_index_names(&prefix, &run_id, mode);

        let manifest = RunManifest {
            run_id: run_id.clone(),
            repo_path: repo.display().to_string(),
            git_commit: git_commit(&repo),
            composer_lock_hash: file_hash(&repo.join("composer.lock"))?,
            indexer_config_hash: config.hash()?,
            framework: framework.as_str().to_string(),
            include_vendor: config.paths.allow_vendor,
            include_tests: config.tests.include_tests,
            mode: mode.as_str().to_string(),
            index_prefix: prefix.clone(),
            indexes: indexes.clone(),
            created_at: Utc::now(),
        };

        let connection = config.resolve_meili()?;
        let meili = MeiliClient::new(connection.clone())?;
        for (suffix, index_name) in &indexes {
            meili.create_index(index_name)?;
            match suffix.as_str() {
                "symbols" => {
                    meili.apply_settings(index_name, &symbols_settings())?;
                    meili.replace_documents(index_name, &symbols)?;
                }
                "routes" => {
                    meili.apply_settings(index_name, &routes_settings())?;
                    meili.replace_documents(index_name, &routes)?;
                }
                "tests" => {
                    meili.apply_settings(index_name, &tests_settings())?;
                    meili.replace_documents(index_name, &tests)?;
                }
                "packages" => {
                    meili.apply_settings(index_name, &packages_settings())?;
                    meili.replace_documents(index_name, &packages_docs)?;
                }
                "schema" => {
                    meili.apply_settings(index_name, &schema_settings())?;
                    meili.replace_documents(index_name, &schema)?;
                }
                "runs" => {
                    meili.apply_settings(index_name, &runs_settings())?;
                    meili.replace_documents(index_name, std::slice::from_ref(&manifest))?;
                }
                _ => {}
            }
        }

        let manifest_path = manifest_path(&repo, &run_id);
        if let Some(parent) = manifest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
        upsert_project_registry(ProjectRecord {
            name: project_name
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| prefix.clone()),
            repo_path: repo.display().to_string(),
            index_prefix: prefix.clone(),
            framework: framework.as_str().to_string(),
            meili_host: connection.host.to_string(),
            last_run_id: run_id.clone(),
            updated_at: Utc::now(),
        })?;

        println!(
            "Indexed {} files into {} ({})\n  symbols: {}\n  routes: {}\n  tests: {}\n  packages: {}\n  schema: {}\n  run: {}",
            files.len(),
            prefix,
            mode.as_str(),
            symbols.len(),
            routes.len(),
            tests.len(),
            packages_docs.len(),
            schema.len(),
            manifest_path.display()
        );
        Ok(())
    }

    pub fn search(
        query: &str,
        project: Option<&str>,
        index: SearchIndex,
        framework: Option<Framework>,
        config_path: &Path,
        json_output: bool,
    ) -> Result<()> {
        load_env_for(config_path);
        let mut config = IndexerConfig::load(config_path)?;
        let current_dir = env::current_dir()?;
        let selected_project = resolve_project_selector(project)?;
        let prefix = selected_project
            .as_ref()
            .map(|item| item.index_prefix.clone())
            .unwrap_or_else(|| config.effective_index_prefix(&current_dir));
        if let Some(project) = selected_project.as_ref() {
            if env::var("MEILI_HOST").is_err() && config.meilisearch.host == "http://127.0.0.1:7700"
            {
                config.meilisearch.host = project.meili_host.clone();
            }
        }
        let meili = MeiliClient::new(config.resolve_meili()?)?;
        let compact = compact_query(query);
        let filter =
            framework.map(|framework| json!([format!("framework = {}", framework.as_str())]));

        match index {
            SearchIndex::All => {
                let symbols = meili.search::<SymbolSearchDoc>(
                    &format!("{prefix}_symbols"),
                    {
                        let mut body = json!({
                            "q": compact,
                            "limit": config.search.exact_limit,
                            "showRankingScore": true,
                            "attributesToSearchOn": ["short_name", "fqn", "owner_class", "symbol_tokens"],
                            "attributesToRetrieve": ["fqn", "path", "line_start", "package_name", "related_tests", "missing_test_warning"],
                            "matchingStrategy": "all",
                            "filter": ["is_test = false"]
                        });
                        if let Some(filter) = &filter {
                            body["filter"] = filter.clone();
                        }
                        body
                    },
                )?;
                let routes = meili.search::<RouteSearchDoc>(
                    &format!("{prefix}_routes"),
                    json!({"q": compact, "limit": config.search.exact_limit, "showRankingScore": true}),
                )?;
                let tests = meili.search::<TestSearchDoc>(
                    &format!("{prefix}_tests"),
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;
                let packages = meili.search::<PackageSearchDoc>(
                    &format!("{prefix}_packages"),
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;
                let schema = meili.search::<SchemaSearchDoc>(
                    &format!("{prefix}_schema"),
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;

                if json_output {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "project": selected_project.as_ref().map(|item| item.name.clone()).unwrap_or(prefix.clone()),
                            "index_prefix": prefix,
                            "symbols": symbols,
                            "routes": routes,
                            "tests": tests,
                            "packages": packages,
                            "schema": schema,
                        }))?
                    );
                } else {
                    if !symbols.hits.is_empty() {
                        println!("Symbols:");
                        for hit in &symbols.hits {
                            println!(
                                "{}\n  path: {}:{}\n  package: {}\n  score: {:?}\n  tests: {}\n",
                                hit.document.fqn,
                                hit.document.path,
                                hit.document.line_start,
                                hit.document.package_name,
                                hit.ranking_score,
                                hit.document.related_tests.join(", ")
                            );
                        }
                    }
                    if !routes.hits.is_empty() {
                        println!("Routes:");
                        for hit in &routes.hits {
                            println!(
                                "{} {} -> {}",
                                hit.document.method,
                                hit.document.uri,
                                hit.document
                                    .action
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string())
                            );
                        }
                    }
                    if !tests.hits.is_empty() {
                        println!("Tests:");
                        for hit in &tests.hits {
                            println!("{} -> {}", hit.document.fqn, hit.document.command);
                        }
                    }
                    if !packages.hits.is_empty() {
                        println!("Packages:");
                        for hit in &packages.hits {
                            println!("{} {:?}", hit.document.name, hit.document.version);
                        }
                    }
                    if !schema.hits.is_empty() {
                        println!("Schema:");
                        for hit in &schema.hits {
                            println!(
                                "{} {:?} {}:{}",
                                hit.document.operation,
                                hit.document.table,
                                hit.document.path,
                                hit.document.line_start
                            );
                        }
                    }
                }
            }
            SearchIndex::Symbols => {
                let index_name = format!("{prefix}_{}", index.suffix());
                let mut body = json!({
                    "q": compact,
                    "limit": config.search.exact_limit,
                    "showRankingScore": true,
                    "attributesToSearchOn": ["short_name", "fqn", "owner_class", "symbol_tokens"],
                    "attributesToRetrieve": [
                        "fqn",
                        "path",
                        "line_start",
                        "package_name",
                        "related_tests",
                        "missing_test_warning"
                    ],
                    "matchingStrategy": "all",
                    "filter": ["is_test = false"]
                });
                if let Some(filter) = filter {
                    body["filter"] = filter;
                }
                let response = meili.search::<SymbolSearchDoc>(&index_name, body)?;
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for hit in response.hits {
                        println!(
                            "{}\n  path: {}:{}\n  package: {}\n  score: {:?}\n  tests: {}\n",
                            hit.document.fqn,
                            hit.document.path,
                            hit.document.line_start,
                            hit.document.package_name,
                            hit.ranking_score,
                            hit.document.related_tests.join(", ")
                        );
                    }
                }
            }
            SearchIndex::Routes => {
                let index_name = format!("{prefix}_{}", index.suffix());
                let response = meili.search::<RouteDoc>(
                    &index_name,
                    json!({"q": compact, "limit": config.search.exact_limit, "showRankingScore": true}),
                )?;
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for hit in response.hits {
                        println!(
                            "{} {} -> {}",
                            hit.document.method,
                            hit.document.uri,
                            hit.document.action.unwrap_or_else(|| "unknown".to_string())
                        );
                    }
                }
            }
            SearchIndex::Tests => {
                let index_name = format!("{prefix}_{}", index.suffix());
                let response = meili.search::<TestDoc>(
                    &index_name,
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for hit in response.hits {
                        println!("{} -> {}", hit.document.fqn, hit.document.command);
                    }
                }
            }
            SearchIndex::Packages => {
                let index_name = format!("{prefix}_{}", index.suffix());
                let response = meili.search::<PackageDoc>(
                    &index_name,
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for hit in response.hits {
                        println!("{} {:?}", hit.document.name, hit.document.version);
                    }
                }
            }
            SearchIndex::Schema => {
                let index_name = format!("{prefix}_{}", index.suffix());
                let response = meili.search::<SchemaDoc>(
                    &index_name,
                    json!({"q": compact, "limit": config.search.natural_limit, "showRankingScore": true}),
                )?;
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for hit in response.hits {
                        println!(
                            "{} {:?} {}:{}",
                            hit.document.operation,
                            hit.document.table,
                            hit.document.path,
                            hit.document.line_start
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn validate(symbol: &str, config_path: &Path, json_output: bool) -> Result<()> {
        load_env_for(config_path);
        let config = IndexerConfig::load(config_path)?;
        let prefix = config.effective_index_prefix(&env::current_dir()?);
        let meili = MeiliClient::new(config.resolve_meili()?)?;
        let response = meili.search::<TestDoc>(
            &format!("{prefix}_tests"),
            json!({
                "q": compact_query(symbol),
                "limit": 10,
                "showRankingScore": true,
                "attributesToSearchOn": ["covered_symbols", "referenced_symbols", "routes_called", "fqn"]
            }),
        )?;
        if json_output {
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }

        let mut hits = response.hits;
        hits.sort_by(|left, right| {
            right
                .document
                .confidence
                .partial_cmp(&left.document.confidence)
                .unwrap()
        });
        println!("Validation for {symbol}");
        let mut strong = 0usize;
        for hit in &hits {
            println!(
                "- {} | confidence {:.2} | {}",
                hit.document.fqn, hit.document.confidence, hit.document.command
            );
            if hit.document.confidence >= config.tests.validate_threshold {
                strong += 1;
            }
        }
        if strong == 0 {
            println!(
                "Validation warning: No related test with confidence >= {:.2} was found.",
                config.tests.validate_threshold
            );
        }
        Ok(())
    }

    pub fn remove(project: &str, keep_indexes: bool, config_path: &Path) -> Result<()> {
        load_env_for(config_path);
        let path = default_project_registry_path();
        let mut registry = ProjectRegistry::load(&path)?;
        let record = registry
            .remove(project)
            .ok_or_else(|| anyhow!("project '{}' not found in {}", project, path.display()))?;

        if !keep_indexes {
            let mut config = IndexerConfig::load(config_path)?;
            if env::var("MEILI_HOST").is_err() && config.meilisearch.host == "http://127.0.0.1:7700"
            {
                config.meilisearch.host = record.meili_host.clone();
            }
            let meili = MeiliClient::new(config.resolve_meili()?)?;
            for suffix in ["symbols", "routes", "tests", "packages", "schema", "runs"] {
                meili.delete_index(&format!("{}_{}", record.index_prefix, suffix))?;
            }
        }

        registry.save(&path)?;
        println!(
            "Removed project '{}' from {}\n  repo: {}\n  indexes_removed: {}",
            record.name,
            path.display(),
            record.repo_path,
            if keep_indexes { "no" } else { "yes" }
        );
        Ok(())
    }

    pub fn verify(config_path: &Path) -> Result<()> {
        load_env_for(config_path);
        let config = IndexerConfig::load(config_path)?;
        let prefix = config.effective_index_prefix(&env::current_dir()?);
        let meili = MeiliClient::new(config.resolve_meili()?)?;
        println!("health {}", meili.health()?);
        for suffix in ["symbols", "routes", "tests", "packages", "schema", "runs"] {
            let stats = meili.stats(&format!("{prefix}_{suffix}"))?;
            let documents = stats
                .get("numberOfDocuments")
                .or_else(|| stats.get("numberOfDocumentsTotal"));
            println!("{suffix:8} docs {:?}", documents);
        }
        let smoke = meili.search::<SymbolDoc>(
            &format!("{prefix}_symbols"),
            json!({"q": "consent", "limit": 3, "showRankingScore": true}),
        )?;
        println!("smoke-search hits {}", smoke.hits.len());
        Ok(())
    }

    pub fn promote(config_path: &Path, run_id: Option<&str>) -> Result<()> {
        load_env_for(config_path);
        let config = IndexerConfig::load(config_path)?;
        let repo = env::current_dir()?;
        let manifest = load_manifest(&repo, run_id)?;
        let meili = MeiliClient::new(config.resolve_meili()?)?;
        let prefix = manifest.index_prefix.clone();

        let mut swaps = Vec::new();
        for suffix in ["symbols", "routes", "tests", "packages", "schema"] {
            let stable = format!("{prefix}_{suffix}");
            let staged = manifest
                .indexes
                .get(suffix)
                .cloned()
                .ok_or_else(|| anyhow!("manifest missing {suffix} index"))?;
            if staged == stable {
                continue;
            }
            meili.create_index(&stable)?;
            swaps.push((stable, staged));
        }
        if swaps.is_empty() {
            println!("No staged indexes to promote");
            return Ok(());
        }
        meili.swap_indexes(swaps)?;
        println!("Promoted run {}", manifest.run_id);
        Ok(())
    }

    fn load_env_for(config_path: &Path) {
        if let Some(root) = config_path.parent().and_then(Path::parent) {
            let env_path = root.join(".env");
            if env_path.exists() {
                let _ = dotenvy::from_path_override(env_path);
            }
        }
    }

    fn command_exists(name: &str) -> bool {
        Command::new("sh")
            .arg("-lc")
            .arg(format!("command -v {name}"))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn file_hash(path: &Path) -> Result<String> {
        if !path.exists() {
            return Ok("missing".to_string());
        }
        let bytes = fs::read(path)?;
        let mut hasher = Sha1::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn git_commit(repo: &Path) -> String {
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn build_index_names(prefix: &str, run_id: &str, mode: IndexMode) -> HashMap<String, String> {
        let mut indexes = HashMap::new();
        for suffix in ["symbols", "routes", "tests", "packages", "schema"] {
            let name = match mode {
                IndexMode::Clean => format!("{prefix}_{suffix}"),
                IndexMode::Staged => format!("{prefix}_{suffix}_tmp_{run_id}"),
            };
            indexes.insert(suffix.to_string(), name);
        }
        indexes.insert("runs".to_string(), format!("{prefix}_runs"));
        indexes
    }

    fn package_docs(repo_name: &str, packages: &ComposerExport) -> Vec<PackageDoc> {
        std::iter::once(&packages.root)
            .chain(packages.packages.iter())
            .map(|package| PackageDoc {
                id: make_stable_id(&[repo_name, &package.name]),
                repo: repo_name.to_string(),
                name: package.name.clone(),
                version: package.version.clone(),
                package_type: package.package_type.clone(),
                description: package.description.clone(),
                install_path: package.install_path.clone(),
                keywords: package.keywords.clone(),
                is_root: package.is_root,
            })
            .collect()
    }

    fn link_routes_to_symbols(symbols: &mut [SymbolDoc], routes: &[RouteDoc]) {
        let route_ids_by_symbol =
            routes
                .iter()
                .fold(HashMap::<String, Vec<String>>::new(), |mut map, route| {
                    for related in &route.related_symbols {
                        map.entry(related.clone())
                            .or_default()
                            .push(route.id.clone());
                    }
                    map
                });
        for symbol in symbols {
            if let Some(route_ids) = route_ids_by_symbol.get(&symbol.fqn) {
                symbol.route_ids = route_ids.clone();
            }
        }
    }

    fn load_manifest(repo: &Path, run_id: Option<&str>) -> Result<RunManifest> {
        let build_dir = repo.join("build/index-runs");
        let path = if let Some(run_id) = run_id {
            build_dir.join(format!("{run_id}.json"))
        } else {
            latest_manifest(&build_dir)?
        };
        serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("parse {}", path.display()))
    }

    fn latest_manifest(dir: &Path) -> Result<PathBuf> {
        let mut manifests = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        manifests.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
        manifests
            .pop()
            .map(|entry| entry.path())
            .ok_or_else(|| anyhow!("no run manifests found in {}", dir.display()))
    }

    fn upsert_project_registry(record: ProjectRecord) -> Result<()> {
        let path = default_project_registry_path();
        let mut registry = ProjectRegistry::load(&path)?;
        registry.upsert(record);
        registry.save(&path)
    }

    fn resolve_project_selector(selector: Option<&str>) -> Result<Option<ProjectRecord>> {
        let Some(selector) = selector else {
            return Ok(None);
        };
        let path = default_project_registry_path();
        let registry = ProjectRegistry::load(&path)?;
        registry
            .resolve(selector)
            .cloned()
            .map(Some)
            .ok_or_else(|| anyhow!("project '{}' not found in {}", selector, path.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::init_workspace_with_connect_path;

    #[test]
    fn init_scaffolds_default_files() {
        let temp = tempdir().unwrap();
        let connect_path = temp.path().join(".config/meilisearch/connect.json");
        init_workspace_with_connect_path(temp.path(), &connect_path, false).unwrap();

        assert!(temp.path().join("config/indexer.toml").exists());
        assert!(temp.path().join(".env.example").exists());
        assert!(temp.path().join("docker-compose.meilisearch.yml").exists());
        assert!(connect_path.exists());
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/indexer.toml"), "existing").unwrap();
        let connect_path = temp.path().join(".config/meilisearch/connect.json");

        let err = init_workspace_with_connect_path(temp.path(), &connect_path, false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn init_does_not_overwrite_existing_global_connect_file() {
        let temp = tempdir().unwrap();
        let connect_path = temp.path().join(".config/meilisearch/connect.json");
        fs::create_dir_all(connect_path.parent().unwrap()).unwrap();
        fs::write(
            &connect_path,
            "{\"url\":\"http://example.test:7700\",\"apiKey\":\"real\"}\n",
        )
        .unwrap();

        init_workspace_with_connect_path(temp.path(), &connect_path, false).unwrap();

        assert_eq!(
            fs::read_to_string(&connect_path).unwrap(),
            "{\"url\":\"http://example.test:7700\",\"apiKey\":\"real\"}\n"
        );
    }
}

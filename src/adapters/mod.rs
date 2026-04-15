mod hyperf;
mod laravel;

use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::Framework;
use crate::models::{RouteDoc, SchemaDoc, make_stable_id};
use crate::sanitizer::Sanitizer;

pub fn detect_framework(repo: &Path, requested: Framework, package_names: &[String]) -> Framework {
    if requested != Framework::Auto {
        return requested;
    }
    if package_names.iter().any(|name| name == "hyperf/hyperf")
        || repo.join("bin/hyperf.php").exists()
    {
        Framework::Hyperf
    } else if package_names.iter().any(|name| name == "laravel/framework")
        || repo.join("artisan").exists()
    {
        Framework::Laravel
    } else {
        Framework::Auto
    }
}

pub fn extract_routes(
    repo: &Path,
    repo_name: &str,
    framework: Framework,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    match framework {
        Framework::Laravel => laravel::extract_routes(repo, repo_name, sanitizer),
        Framework::Hyperf => hyperf::extract_routes(repo, repo_name, sanitizer),
        Framework::Auto => Ok(Vec::new()),
    }
}

pub fn extract_schema(repo: &Path, repo_name: &str) -> Result<Vec<SchemaDoc>> {
    let migrations = repo.join("database/migrations");
    if !migrations.exists() {
        return Ok(Vec::new());
    }
    let create_re = Regex::new(r#"Schema::create\(\s*['"]([^'"]+)['"]"#).unwrap();
    let table_re = Regex::new(r#"Schema::table\(\s*['"]([^'"]+)['"]"#).unwrap();
    let mut docs = Vec::new();
    for entry in walkdir::WalkDir::new(&migrations)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let contents = fs::read_to_string(entry.path())?;
        for (idx, line) in contents.lines().enumerate() {
            let capture = create_re.captures(line).or_else(|| table_re.captures(line));
            if let Some(capture) = capture {
                let table = capture.get(1).unwrap().as_str().to_string();
                let operation = if line.contains("Schema::create") {
                    "create"
                } else {
                    "table"
                };
                let path = entry
                    .path()
                    .strip_prefix(repo)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                docs.push(SchemaDoc {
                    id: make_stable_id(&[repo_name, &path, &table, operation]),
                    repo: repo_name.to_string(),
                    migration: entry.file_name().to_string_lossy().into_owned(),
                    table: Some(table),
                    operation: operation.to_string(),
                    path,
                    line_start: idx + 1,
                });
            }
        }
    }
    Ok(docs)
}

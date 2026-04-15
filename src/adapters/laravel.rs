use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use regex::Regex;
use serde::Deserialize;

use crate::models::{RouteDoc, make_stable_id};
use crate::sanitizer::Sanitizer;

pub fn extract_routes(
    repo: &Path,
    repo_name: &str,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    if let Ok(routes) = artisan_routes(repo, repo_name) {
        return Ok(routes);
    }
    static_routes(repo, repo_name, sanitizer)
}

fn artisan_routes(repo: &Path, repo_name: &str) -> Result<Vec<RouteDoc>> {
    #[derive(Debug, Deserialize)]
    struct ArtisanRoute {
        method: String,
        uri: String,
        name: Option<String>,
        action: Option<String>,
        #[serde(default)]
        middleware: Vec<String>,
    }

    let output = Command::new("php")
        .arg("artisan")
        .arg("route:list")
        .arg("--json")
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("artisan route:list failed");
    }
    let routes: Vec<ArtisanRoute> = serde_json::from_slice(&output.stdout)?;
    Ok(routes
        .into_iter()
        .map(|route| {
            let action = route.action.clone();
            let (controller, controller_method) = split_action(action.as_deref());
            RouteDoc {
                id: make_stable_id(&[repo_name, "laravel", &route.method, &route.uri]),
                repo: repo_name.to_string(),
                framework: "laravel".to_string(),
                method: route.method,
                uri: route.uri,
                route_name: route.name,
                action,
                controller,
                controller_method,
                middleware: route.middleware,
                related_symbols: Vec::new(),
                related_tests: Vec::new(),
                package_name: "root/app".to_string(),
                path: None,
                line_start: None,
            }
        })
        .collect())
}

fn static_routes(repo: &Path, repo_name: &str, sanitizer: &Sanitizer) -> Result<Vec<RouteDoc>> {
    let routes_dir = repo.join("routes");
    if !routes_dir.exists() {
        return Ok(Vec::new());
    }
    let route_re = Regex::new(
        r#"Route::(get|post|put|patch|delete|options|any)\(\s*['"]([^'"]+)['"]\s*,\s*\[?\s*([A-Za-z0-9_\\]+)::class\s*,\s*['"]([A-Za-z0-9_]+)['"]"#,
    )
    .unwrap();
    let name_re = Regex::new(r#"->name\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    let middleware_re = Regex::new(r#"->middleware\(([^)]+)\)"#).unwrap();

    let mut docs = Vec::new();
    for entry in walkdir::WalkDir::new(&routes_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let contents = fs::read_to_string(entry.path())?;
        let path = entry
            .path()
            .strip_prefix(repo)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for (idx, line) in contents.lines().enumerate() {
            let Some(caps) = route_re.captures(line) else {
                continue;
            };
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let uri = sanitizer
                .sanitize_text(caps.get(2).unwrap().as_str())
                .unwrap_or_else(|| "/redacted".to_string());
            let controller = caps.get(3).unwrap().as_str().to_string();
            let controller_method = caps.get(4).unwrap().as_str().to_string();
            let route_name = name_re
                .captures(line)
                .and_then(|caps| caps.get(1))
                .map(|item| item.as_str().to_string());
            let middleware = middleware_re
                .captures(line)
                .and_then(|caps| caps.get(1))
                .map(|item| {
                    item.as_str()
                        .split(',')
                        .map(|part| {
                            part.trim_matches(|c| {
                                c == '\'' || c == '"' || c == ' ' || c == '[' || c == ']'
                            })
                        })
                        .filter(|part| !part.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            docs.push(RouteDoc {
                id: make_stable_id(&[
                    repo_name,
                    "laravel",
                    &method,
                    &uri,
                    &path,
                    &(idx + 1).to_string(),
                ]),
                repo: repo_name.to_string(),
                framework: "laravel".to_string(),
                method,
                uri,
                route_name,
                action: Some(format!("{controller}@{controller_method}")),
                controller: Some(controller.clone()),
                controller_method: Some(controller_method.clone()),
                middleware,
                related_symbols: vec![format!("{controller}::{controller_method}")],
                related_tests: Vec::new(),
                package_name: "root/app".to_string(),
                path: Some(path.clone()),
                line_start: Some(idx + 1),
            });
        }
    }
    Ok(docs)
}

fn split_action(action: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(action) = action else {
        return (None, None);
    };
    if let Some((controller, method)) = action.split_once('@') {
        (Some(controller.to_string()), Some(method.to_string()))
    } else {
        (Some(action.to_string()), None)
    }
}

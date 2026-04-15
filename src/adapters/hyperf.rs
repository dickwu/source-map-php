use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::models::{RouteDoc, make_stable_id};
use crate::sanitizer::Sanitizer;

pub fn extract_routes(
    repo: &Path,
    repo_name: &str,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    let mut routes = Vec::new();
    let config_routes = repo.join("config/routes.php");
    if config_routes.exists() {
        routes.extend(parse_config_routes(
            repo,
            repo_name,
            &config_routes,
            sanitizer,
        )?);
    }
    routes.extend(parse_attribute_routes(repo, repo_name, sanitizer)?);
    Ok(routes)
}

fn parse_config_routes(
    repo: &Path,
    repo_name: &str,
    path: &Path,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    let route_re = Regex::new(
        r#"Router::addRoute\(\s*\[?['"]?([A-Z]+)['"]?\]?\s*,\s*['"]([^'"]+)['"]\s*,\s*\[([A-Za-z0-9_\\]+)::class\s*,\s*['"]([A-Za-z0-9_]+)['"]"#,
    )
    .unwrap();
    let contents = fs::read_to_string(path)?;
    let rel = path
        .strip_prefix(repo)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut docs = Vec::new();
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
        docs.push(RouteDoc {
            id: make_stable_id(&[
                repo_name,
                "hyperf",
                &method,
                &uri,
                &rel,
                &(idx + 1).to_string(),
            ]),
            repo: repo_name.to_string(),
            framework: "hyperf".to_string(),
            method,
            uri,
            route_name: None,
            action: Some(format!("{controller}@{controller_method}")),
            controller: Some(controller.clone()),
            controller_method: Some(controller_method.clone()),
            middleware: Vec::new(),
            related_symbols: vec![format!("{controller}::{controller_method}")],
            related_tests: Vec::new(),
            package_name: "root/app".to_string(),
            path: Some(rel.clone()),
            line_start: Some(idx + 1),
        });
    }
    Ok(docs)
}

fn parse_attribute_routes(
    repo: &Path,
    repo_name: &str,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    let attr_re = Regex::new(
        r#"#\[(GetMapping|PostMapping|PutMapping|DeleteMapping|RequestMapping)\((?:path:\s*)?['"]([^'"]+)['"]"#,
    )
    .unwrap();
    let fn_re = Regex::new(r#"function\s+([A-Za-z0-9_]+)\s*\("#).unwrap();
    let class_re = Regex::new(r#"class\s+([A-Za-z0-9_]+)"#).unwrap();
    let namespace_re = Regex::new(r#"namespace\s+([^;]+);"#).unwrap();

    let mut docs = Vec::new();
    for entry in walkdir::WalkDir::new(repo.join("app"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let contents = fs::read_to_string(entry.path())?;
        let namespace = namespace_re
            .captures(&contents)
            .and_then(|caps| caps.get(1))
            .map(|item| item.as_str().to_string());
        let class = class_re
            .captures(&contents)
            .and_then(|caps| caps.get(1))
            .map(|item| item.as_str().to_string());
        let rel = entry
            .path()
            .strip_prefix(repo)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let lines: Vec<_> = contents.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let Some(caps) = attr_re.captures(line) else {
                continue;
            };
            let method = match caps.get(1).unwrap().as_str() {
                "GetMapping" => "GET",
                "PostMapping" => "POST",
                "PutMapping" => "PUT",
                "DeleteMapping" => "DELETE",
                _ => "ANY",
            }
            .to_string();
            let uri = sanitizer
                .sanitize_text(caps.get(2).unwrap().as_str())
                .unwrap_or_else(|| "/redacted".to_string());
            let controller_method = lines
                .iter()
                .skip(idx + 1)
                .find_map(|candidate| fn_re.captures(candidate))
                .and_then(|caps| caps.get(1))
                .map(|item| item.as_str().to_string());
            let controller = class.as_ref().map(|class| match &namespace {
                Some(namespace) => format!("{namespace}\\{class}"),
                None => class.clone(),
            });

            docs.push(RouteDoc {
                id: make_stable_id(&[
                    repo_name,
                    "hyperf",
                    &method,
                    &uri,
                    &rel,
                    &(idx + 1).to_string(),
                ]),
                repo: repo_name.to_string(),
                framework: "hyperf".to_string(),
                method,
                uri,
                route_name: None,
                action: controller
                    .as_ref()
                    .zip(controller_method.as_ref())
                    .map(|(controller, method)| format!("{controller}@{method}")),
                controller: controller.clone(),
                controller_method: controller_method.clone(),
                middleware: Vec::new(),
                related_symbols: controller
                    .as_ref()
                    .zip(controller_method.as_ref())
                    .map(|(controller, method)| vec![format!("{controller}::{method}")])
                    .unwrap_or_default(),
                related_tests: Vec::new(),
                package_name: "root/app".to_string(),
                path: Some(rel.clone()),
                line_start: Some(idx + 1),
            });
        }
    }
    Ok(docs)
}

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::models::{RouteDoc, make_stable_id};
use crate::sanitizer::Sanitizer;

#[derive(Debug, Clone, Default)]
struct GroupContext {
    prefix: String,
    middleware: Vec<String>,
    doc_start_index: usize,
}

pub fn extract_routes(
    repo: &Path,
    repo_name: &str,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    let mut routes = Vec::new();
    let config_root = repo.join("config");
    if !config_root.exists() {
        return Ok(routes);
    }

    for entry in walkdir::WalkDir::new(&config_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("php"))
    {
        routes.extend(parse_route_file(repo, repo_name, entry.path(), sanitizer)?);
    }

    routes.extend(parse_attribute_routes(repo, repo_name, sanitizer)?);
    Ok(routes)
}

fn parse_route_file(
    repo: &Path,
    repo_name: &str,
    path: &Path,
    sanitizer: &Sanitizer,
) -> Result<Vec<RouteDoc>> {
    let add_route_re = Regex::new(
        r#"Router::addRoute\(\s*(\[[^\]]+\]|['"][A-Z,]+['"])\s*,\s*['"]([^'"]+)['"]\s*,\s*\[([A-Za-z0-9_\\]+)::class\s*,\s*['"]([A-Za-z0-9_]+)['"]"#,
    )
    .unwrap();
    let verb_route_re = Regex::new(
        r#"Router::(get|post|put|patch|delete|options|any)\(\s*['"]([^'"]+)['"]\s*,\s*\[([A-Za-z0-9_\\]+)::class\s*,\s*['"]([A-Za-z0-9_]+)['"]"#,
    )
    .unwrap();
    let add_group_re =
        Regex::new(r#"Router::addGroup\(\s*['"]([^'"]+)['"]\s*,\s*static function"#).unwrap();
    let use_re = Regex::new(r#"^use\s+([^;]+);"#).unwrap();
    let middleware_re = Regex::new(r#"'middleware'\s*=>\s*\[([^\]]*)\]"#).unwrap();

    let contents = fs::read_to_string(path)?;
    let rel = path
        .strip_prefix(repo)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let lines: Vec<_> = contents.lines().collect();
    let imports = collect_imports(&contents, &use_re);
    let mut docs = Vec::new();
    let mut groups: Vec<GroupContext> = Vec::new();
    let mut pending_group: Option<GroupContext> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(caps) = add_group_re.captures(trimmed) {
            pending_group = Some(GroupContext {
                prefix: caps.get(1).unwrap().as_str().to_string(),
                middleware: middleware_re
                    .captures(trimmed)
                    .and_then(|caps| caps.get(1))
                    .map(|item| parse_middleware_list(item.as_str()))
                    .unwrap_or_default(),
                doc_start_index: docs.len(),
            });
        }

        if trimmed.ends_with('{')
            && let Some(group) = pending_group.take()
        {
            groups.push(group);
        }

        if let Some(caps) = add_route_re.captures(trimmed) {
            let methods = parse_methods(caps.get(1).unwrap().as_str());
            let uri = build_uri(
                groups.iter().map(|group| group.prefix.as_str()),
                caps.get(2).unwrap().as_str(),
            );
            let controller = resolve_controller(caps.get(3).unwrap().as_str(), &imports);
            let controller_method = caps.get(4).unwrap().as_str().to_string();
            docs.extend(build_route_docs(
                repo_name,
                "hyperf",
                methods,
                &uri,
                &controller,
                &controller_method,
                &rel,
                idx + 1,
                sanitizer,
                groups
                    .iter()
                    .flat_map(|group| group.middleware.iter())
                    .cloned()
                    .collect(),
            ));
        } else if let Some(caps) = verb_route_re.captures(trimmed) {
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let uri = build_uri(
                groups.iter().map(|group| group.prefix.as_str()),
                caps.get(2).unwrap().as_str(),
            );
            let controller = resolve_controller(caps.get(3).unwrap().as_str(), &imports);
            let controller_method = caps.get(4).unwrap().as_str().to_string();
            docs.extend(build_route_docs(
                repo_name,
                "hyperf",
                vec![method],
                &uri,
                &controller,
                &controller_method,
                &rel,
                idx + 1,
                sanitizer,
                groups
                    .iter()
                    .flat_map(|group| group.middleware.iter())
                    .cloned()
                    .collect(),
            ));
        }

        if (trimmed.starts_with("},") || trimmed.starts_with("});") || trimmed == "});")
            && let Some(mut group) = groups.pop()
        {
            if group.middleware.is_empty()
                && let Some(caps) = middleware_re.captures(trimmed)
            {
                group.middleware = parse_middleware_list(caps.get(1).unwrap().as_str());
            }
            if !group.middleware.is_empty() {
                for route in docs.iter_mut().skip(group.doc_start_index) {
                    for middleware in &group.middleware {
                        if !route.middleware.iter().any(|item| item == middleware) {
                            route.middleware.push(middleware.clone());
                        }
                    }
                }
            }
        }
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
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("php"))
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

fn collect_imports(contents: &str, use_re: &Regex) -> HashMap<String, String> {
    contents
        .lines()
        .filter_map(|line| {
            let captures = use_re.captures(line.trim())?;
            let import = captures.get(1)?.as_str().trim().to_string();
            let short = import.rsplit('\\').next()?.to_string();
            Some((short, import))
        })
        .collect()
}

fn parse_middleware_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_end_matches("::class").to_string())
        .collect()
}

fn parse_methods(raw: &str) -> Vec<String> {
    raw.trim_matches(|c| c == '[' || c == ']' || c == '\'' || c == '"' || c == ' ')
        .split(',')
        .map(|part| part.trim_matches(|c| c == '\'' || c == '"' || c == ' '))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_uppercase())
        .collect()
}

fn resolve_controller(raw: &str, imports: &HashMap<String, String>) -> String {
    imports.get(raw).cloned().unwrap_or_else(|| raw.to_string())
}

fn build_uri<'a>(prefixes: impl Iterator<Item = &'a str>, raw_uri: &str) -> String {
    let mut parts = prefixes
        .map(|prefix| prefix.trim_matches('/'))
        .filter(|prefix| !prefix.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let uri = raw_uri.trim_matches('/');
    if !uri.is_empty() {
        parts.push(uri.to_string());
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

#[allow(clippy::too_many_arguments)]
fn build_route_docs(
    repo_name: &str,
    framework: &str,
    methods: Vec<String>,
    uri: &str,
    controller: &str,
    controller_method: &str,
    rel: &str,
    line: usize,
    sanitizer: &Sanitizer,
    middleware: Vec<String>,
) -> Vec<RouteDoc> {
    let safe_uri = sanitizer
        .sanitize_text(uri)
        .unwrap_or_else(|| "/redacted".to_string());
    methods
        .into_iter()
        .map(|method| RouteDoc {
            id: make_stable_id(&[
                repo_name,
                framework,
                &method,
                &safe_uri,
                rel,
                &line.to_string(),
            ]),
            repo: repo_name.to_string(),
            framework: framework.to_string(),
            method,
            uri: safe_uri.clone(),
            route_name: None,
            action: Some(format!("{controller}@{controller_method}")),
            controller: Some(controller.to_string()),
            controller_method: Some(controller_method.to_string()),
            middleware: middleware.clone(),
            related_symbols: vec![format!("{controller}::{controller_method}")],
            related_tests: Vec::new(),
            package_name: "root/app".to_string(),
            path: Some(rel.to_string()),
            line_start: Some(line),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::sanitizer::Sanitizer;

    use super::parse_route_file;

    #[test]
    fn parses_grouped_hyperf_routes_from_router_files() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("config/routers");
        fs::create_dir_all(&config_dir).unwrap();
        let path = config_dir.join("patient.php");
        fs::write(
            &path,
            r#"<?php
use App\Controller\Patient\PatientMainController;
use App\Middleware\AuthToken;
use Hyperf\HttpServer\Router\Router;

Router::addGroup('/patient-main', static function () {
    Router::post('/list', [PatientMainController::class, 'list']);
    Router::post('/detail', [PatientMainController::class, 'detail']);
}, ['middleware' => [AuthToken::class]]);
"#,
        )
        .unwrap();
        fs::write(config_dir.join(".DS_Store"), b"not-php").unwrap();

        let routes =
            parse_route_file(temp.path(), "acme/staff-api", &path, &Sanitizer::default()).unwrap();

        assert_eq!(routes.len(), 2);
        assert!(routes.iter().any(|route| {
            route.uri == "/patient-main/list"
                && route.action.as_deref()
                    == Some("App\\Controller\\Patient\\PatientMainController@list")
        }));
        assert!(
            routes
                .iter()
                .all(|route| route.middleware.iter().any(|item| item == "AuthToken"))
        );
    }
}

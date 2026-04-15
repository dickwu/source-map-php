mod fallback;
mod phpactor;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::Framework;
use crate::composer::ComposerExport;
use crate::models::{SymbolDoc, make_stable_id};
use crate::sanitizer::Sanitizer;

#[derive(Debug, Clone)]
pub struct DeclarationCandidate {
    pub kind: String,
    pub name: String,
    pub owner_class: Option<String>,
    pub namespace: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub extraction_confidence: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedComments {
    summary: Option<String>,
    description: Option<String>,
    params: Vec<String>,
    return_doc: Option<String>,
    throws_docs: Vec<String>,
    inline_comments: Vec<String>,
}

pub fn extract_symbols(
    repo: &Path,
    repo_name: &str,
    framework: Framework,
    files: &[crate::scanner::ScannedFile],
    packages: &ComposerExport,
    sanitizer: &Sanitizer,
) -> Result<Vec<SymbolDoc>> {
    let mut phpactor = phpactor::PhpactorExtractor::connect(repo).ok();
    let mut symbols = Vec::new();

    for file in files {
        if !file.relative_path.to_string_lossy().ends_with(".php") {
            continue;
        }

        let contents = fs::read_to_string(&file.absolute_path)?;
        let declarations = if let Some(client) = phpactor.as_mut() {
            client
                .extract_candidates(&file.absolute_path, &contents)
                .unwrap_or_else(|_| fallback::extract_candidates(&contents))
        } else {
            fallback::extract_candidates(&contents)
        };
        let comment_map = collect_comments(&contents, sanitizer);
        let package = packages.package_for_path(&file.absolute_path);
        let is_test =
            file.relative_path.starts_with("tests") || file.relative_path.starts_with("test");
        let path_str = file.relative_path.to_string_lossy().into_owned();
        let abs_str = file.absolute_path.to_string_lossy().into_owned();

        for declaration in declarations {
            let fqn = build_fqn(&declaration);
            let stable_key = format!("{}|{}|{}", repo_name, declaration.kind, fqn);
            let comments = comment_map
                .get(&declaration.line_start)
                .cloned()
                .unwrap_or_default();

            symbols.push(SymbolDoc {
                id: make_stable_id(&[
                    repo_name,
                    &declaration.kind,
                    &fqn,
                    &path_str,
                    &declaration.line_start.to_string(),
                ]),
                stable_key,
                repo: repo_name.to_string(),
                framework: framework.as_str().to_string(),
                kind: declaration.kind.clone(),
                short_name: declaration.name.clone(),
                fqn,
                owner_class: declaration.owner_class.clone(),
                namespace: declaration.namespace.clone(),
                signature: declaration.signature.clone(),
                doc_summary: comments.summary.clone(),
                doc_description: comments.description.clone(),
                param_docs: comments.params.clone(),
                return_doc: comments.return_doc.clone(),
                throws_docs: comments.throws_docs.clone(),
                magic_methods: Vec::new(),
                magic_properties: Vec::new(),
                inline_rule_comments: comments.inline_comments.clone(),
                comment_keywords: keywordize(
                    comments
                        .summary
                        .iter()
                        .chain(comments.inline_comments.iter())
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                        .as_str(),
                ),
                symbol_tokens: keywordize(&declaration.name),
                framework_tags: vec![framework.as_str().to_string()],
                risk_tags: infer_risk_tags(&path_str, comments.summary.as_deref()),
                route_ids: Vec::new(),
                related_symbols: Vec::new(),
                related_tests: Vec::new(),
                related_tests_count: 0,
                references_count: 0,
                validation_commands: Vec::new(),
                missing_test_warning: None,
                package_name: package.name.clone(),
                package_type: package.package_type.clone(),
                package_version: package.version.clone(),
                package_keywords: package.keywords.clone(),
                is_vendor: !package.is_root,
                is_project_code: package.is_root,
                is_test,
                autoloadable: true,
                extraction_confidence: declaration.extraction_confidence.clone(),
                path: path_str.clone(),
                absolute_path: abs_str.clone(),
                line_start: declaration.line_start,
                line_end: declaration.line_end,
            });
        }
    }

    Ok(symbols)
}

fn build_fqn(declaration: &DeclarationCandidate) -> String {
    match (&declaration.namespace, &declaration.owner_class) {
        (Some(namespace), Some(owner)) if declaration.kind == "method" => {
            format!("{namespace}\\{owner}::{}", declaration.name)
        }
        (Some(namespace), _) => format!("{namespace}\\{}", declaration.name),
        (None, Some(owner)) if declaration.kind == "method" => {
            format!("{owner}::{}", declaration.name)
        }
        _ => declaration.name.clone(),
    }
}

fn collect_comments(contents: &str, sanitizer: &Sanitizer) -> HashMap<usize, ParsedComments> {
    let mut map = HashMap::new();
    let lines: Vec<_> = contents.lines().collect();
    let decl_re = Regex::new(r"^\s*(?:final\s+|abstract\s+)?(?:class|interface|trait|enum|function|public\s+function|protected\s+function|private\s+function)").unwrap();
    let param_re = Regex::new(r"@param\s+(.+)").unwrap();
    let return_re = Regex::new(r"@return\s+(.+)").unwrap();
    let throws_re = Regex::new(r"@throws\s+(.+)").unwrap();

    for (idx, line) in lines.iter().enumerate() {
        if !decl_re.is_match(line) {
            continue;
        }
        let mut cursor = idx as isize - 1;
        let mut doc_lines = Vec::new();
        let mut inline_comments = Vec::new();
        while cursor >= 0 {
            let candidate = lines[cursor as usize].trim();
            if candidate.starts_with("//") || candidate.starts_with('#') {
                if let Some(value) =
                    sanitizer.sanitize_text(candidate.trim_start_matches(&['/', '#'][..]).trim())
                {
                    inline_comments.push(value);
                }
                cursor -= 1;
                continue;
            }
            if candidate.ends_with("*/")
                || candidate.starts_with('*')
                || candidate.starts_with("/**")
            {
                doc_lines.push(candidate.to_string());
                cursor -= 1;
                continue;
            }
            break;
        }
        doc_lines.reverse();
        inline_comments.reverse();

        let mut parsed = ParsedComments::default();
        let mut description_lines = Vec::new();
        for raw in doc_lines {
            let cleaned = raw
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim();
            if cleaned.is_empty() {
                continue;
            }
            if let Some(param) = param_re
                .captures(cleaned)
                .and_then(|caps| caps.get(1).map(|item| item.as_str()))
                .and_then(|value| sanitizer.sanitize_text(value))
            {
                parsed.params.push(param);
                continue;
            }
            if let Some(return_doc) = return_re
                .captures(cleaned)
                .and_then(|caps| caps.get(1).map(|item| item.as_str()))
                .and_then(|value| sanitizer.sanitize_text(value))
            {
                parsed.return_doc = Some(return_doc);
                continue;
            }
            if let Some(throws_doc) = throws_re
                .captures(cleaned)
                .and_then(|caps| caps.get(1).map(|item| item.as_str()))
                .and_then(|value| sanitizer.sanitize_text(value))
            {
                parsed.throws_docs.push(throws_doc);
                continue;
            }
            if parsed.summary.is_none() {
                parsed.summary = sanitizer.sanitize_text(cleaned);
            } else if let Some(line) = sanitizer.sanitize_text(cleaned) {
                description_lines.push(line);
            }
        }
        parsed.description = if description_lines.is_empty() {
            None
        } else {
            Some(description_lines.join(" "))
        };
        parsed.inline_comments = inline_comments;

        map.insert(idx + 1, parsed);
    }

    map
}

fn keywordize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '\\')
        .filter(|token| token.len() > 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn infer_risk_tags(path: &str, summary: Option<&str>) -> Vec<String> {
    let mut tags = Vec::new();
    let text = format!("{path} {}", summary.unwrap_or_default()).to_ascii_lowercase();
    for (needle, tag) in [
        ("policy", "risk:access-control"),
        ("auth", "risk:access-control"),
        ("consent", "risk:patient-consent"),
        ("audit", "risk:audit-trail"),
        ("patient", "risk:patient-data"),
    ] {
        if text.contains(needle) {
            tags.push(tag.to_string());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::Framework;
    use crate::composer::export_packages;
    use crate::config::IndexerConfig;
    use crate::sanitizer::Sanitizer;
    use crate::scanner::scan_repo;

    use super::extract_symbols;

    #[test]
    fn extracts_symbols_with_docblocks() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app")).unwrap();
        fs::write(dir.path().join("composer.json"), r#"{"name":"acme/app"}"#).unwrap();
        fs::write(
            dir.path().join("app/ConsentService.php"),
            r#"<?php
namespace App\Services;

class ConsentService {
    /**
     * Sign consent.
     * @param string $patientId patient id
     * @return bool
     */
    public function sign(string $patientId): bool
    {
        return true;
    }
}
"#,
        )
        .unwrap();

        let files = scan_repo(dir.path(), &IndexerConfig::default().paths).unwrap();
        let packages = export_packages(dir.path()).unwrap();
        let symbols = extract_symbols(
            dir.path(),
            "acme/app",
            Framework::Laravel,
            &files,
            &packages,
            &Sanitizer::default(),
        )
        .unwrap();

        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.fqn == "App\\Services\\ConsentService")
        );
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.fqn == "App\\Services\\ConsentService::sign"
                    && symbol.doc_summary.as_deref() == Some("Sign consent."))
        );
    }
}

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::models::{RouteDoc, SymbolDoc, TestDoc, make_stable_id};
use crate::{Framework, config::TestsConfig};

pub fn extract_tests(
    _repo: &Path,
    repo_name: &str,
    framework: Framework,
    files: &[crate::scanner::ScannedFile],
) -> Result<Vec<TestDoc>> {
    let covers_class_re = Regex::new(r#"#\[CoversClass\(([^)]+)::class\)\]"#).unwrap();
    let covers_method_re =
        Regex::new(r#"#\[CoversMethod\(([^)]+)::class,\s*['"]([A-Za-z0-9_]+)['"]\)\]"#).unwrap();
    let covers_doc_re = Regex::new(r#"@covers\s+\\?([A-Za-z0-9_\\:]+)"#).unwrap();
    let route_call_re = Regex::new(
        r#"(?:->|::)(?:get|post|put|patch|delete|json)\(\s*(?:['"][A-Z]+['"]\s*,\s*)?['"]([^'"]+)['"]"#,
    )
    .unwrap();
    let test_name_re = Regex::new(r#"function\s+([A-Za-z0-9_]+)\s*\("#).unwrap();

    let mut docs = Vec::new();
    for file in files.iter().filter(|file| {
        file.relative_path.starts_with("tests") || file.relative_path.starts_with("test")
    }) {
        let contents = fs::read_to_string(&file.absolute_path)?;
        let fqn = test_name_re
            .captures(&contents)
            .and_then(|caps| caps.get(1))
            .map(|item| item.as_str().to_string())
            .unwrap_or_else(|| {
                file.relative_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("UnknownTest")
                    .to_string()
            });
        let covered_symbols = covers_class_re
            .captures_iter(&contents)
            .filter_map(|caps| caps.get(1).map(|item| item.as_str().to_string()))
            .chain(
                covers_method_re
                    .captures_iter(&contents)
                    .filter_map(|caps| {
                        Some(format!(
                            "{}::{}",
                            caps.get(1)?.as_str(),
                            caps.get(2)?.as_str()
                        ))
                    }),
            )
            .chain(
                covers_doc_re
                    .captures_iter(&contents)
                    .filter_map(|caps| caps.get(1).map(|item| item.as_str().to_string())),
            )
            .collect::<Vec<_>>();
        let routes_called = route_call_re
            .captures_iter(&contents)
            .filter_map(|caps| caps.get(1).map(|item| item.as_str().to_string()))
            .collect::<Vec<_>>();
        let command = match framework {
            Framework::Hyperf => format!("vendor/bin/co-phpunit --filter {fqn}"),
            _ => format!("php artisan test --filter {fqn}"),
        };
        let path = file.relative_path.to_string_lossy().into_owned();
        docs.push(TestDoc {
            id: make_stable_id(&[repo_name, &path, &fqn]),
            repo: repo_name.to_string(),
            framework: framework.as_str().to_string(),
            fqn,
            path,
            line_start: 1,
            covered_symbols,
            referenced_symbols: Vec::new(),
            routes_called,
            command,
            confidence: 0.0,
        });
    }
    Ok(docs)
}

pub fn link_symbols_and_routes(
    symbols: &mut [SymbolDoc],
    routes: &mut [RouteDoc],
    tests: &mut [TestDoc],
    test_config: &TestsConfig,
) {
    let mut test_scores: HashMap<String, Vec<(usize, f32)>> = HashMap::new();

    for (test_index, test) in tests.iter_mut().enumerate() {
        let mut best = 0.0f32;
        for symbol in symbols.iter() {
            let score = score_test_against_symbol(test, symbol);
            if score > 0.0 {
                test_scores
                    .entry(symbol.id.clone())
                    .or_default()
                    .push((test_index, score));
                if score > best {
                    best = score;
                }
            }
        }
        test.confidence = best;
    }

    for symbol in symbols {
        let Some(scored_tests) = test_scores.get(&symbol.id) else {
            symbol.missing_test_warning = Some(format!(
                "No related test with confidence >= {:.2} was found.",
                test_config.validate_threshold
            ));
            continue;
        };

        let mut scored = scored_tests.clone();
        scored.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap());
        symbol.related_tests = scored
            .iter()
            .map(|(index, _)| tests[*index].fqn.clone())
            .collect();
        symbol.related_tests_count = symbol.related_tests.len() as u32;
        symbol.validation_commands = scored
            .iter()
            .filter(|(_, score)| *score >= test_config.validate_threshold)
            .map(|(index, _)| tests[*index].command.clone())
            .collect();
        if symbol.validation_commands.is_empty() {
            symbol.missing_test_warning = Some(format!(
                "No related test with confidence >= {:.2} was found.",
                test_config.validate_threshold
            ));
        }
    }

    for route in routes {
        route.related_tests = tests
            .iter()
            .filter(|test| test.routes_called.iter().any(|called| called == &route.uri))
            .map(|test| test.fqn.clone())
            .collect();
    }
}

fn score_test_against_symbol(test: &TestDoc, symbol: &SymbolDoc) -> f32 {
    if test
        .covered_symbols
        .iter()
        .any(|covered| covered == &symbol.fqn)
    {
        return 0.95;
    }
    if test
        .covered_symbols
        .iter()
        .any(|covered| covered.ends_with(&symbol.short_name))
    {
        return 0.8;
    }
    if test
        .routes_called
        .iter()
        .any(|called| symbol.path.contains(called.trim_matches('/')))
    {
        return 0.65;
    }
    0.0
}

#[cfg(test)]
mod tests {
    use crate::config::TestsConfig;
    use crate::models::{SymbolDoc, TestDoc};

    use super::link_symbols_and_routes;

    fn symbol() -> SymbolDoc {
        SymbolDoc {
            id: "symbol".to_string(),
            stable_key: "symbol".to_string(),
            repo: "repo".to_string(),
            framework: "laravel".to_string(),
            kind: "method".to_string(),
            short_name: "sign".to_string(),
            fqn: "App\\Services\\ConsentService::sign".to_string(),
            owner_class: Some("App\\Services\\ConsentService".to_string()),
            namespace: Some("App\\Services".to_string()),
            signature: None,
            doc_summary: None,
            doc_description: None,
            param_docs: Vec::new(),
            return_doc: None,
            throws_docs: Vec::new(),
            magic_methods: Vec::new(),
            magic_properties: Vec::new(),
            inline_rule_comments: Vec::new(),
            comment_keywords: Vec::new(),
            symbol_tokens: Vec::new(),
            framework_tags: Vec::new(),
            risk_tags: Vec::new(),
            route_ids: Vec::new(),
            related_symbols: Vec::new(),
            related_tests: Vec::new(),
            related_tests_count: 0,
            references_count: 0,
            validation_commands: Vec::new(),
            missing_test_warning: None,
            package_name: "root/app".to_string(),
            package_type: None,
            package_version: None,
            package_keywords: Vec::new(),
            is_vendor: false,
            is_project_code: true,
            is_test: false,
            autoloadable: true,
            extraction_confidence: "fallback".to_string(),
            path: "app/Services/ConsentService.php".to_string(),
            absolute_path: "/repo/app/Services/ConsentService.php".to_string(),
            line_start: 10,
            line_end: 20,
        }
    }

    #[test]
    fn adds_validation_commands_for_high_confidence_tests() {
        let mut symbols = vec![symbol()];
        let mut tests = vec![TestDoc {
            id: "test".to_string(),
            repo: "repo".to_string(),
            framework: "laravel".to_string(),
            fqn: "PatientConsentTest".to_string(),
            path: "tests/Feature/PatientConsentTest.php".to_string(),
            line_start: 1,
            covered_symbols: vec!["App\\Services\\ConsentService::sign".to_string()],
            referenced_symbols: Vec::new(),
            routes_called: Vec::new(),
            command: "php artisan test --filter PatientConsentTest".to_string(),
            confidence: 0.0,
        }];

        link_symbols_and_routes(&mut symbols, &mut [], &mut tests, &TestsConfig::default());

        assert_eq!(
            symbols[0].validation_commands,
            vec!["php artisan test --filter PatientConsentTest"]
        );
        assert!(symbols[0].missing_test_warning.is_none());
    }
}

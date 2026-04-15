use std::path::Path;

use source_map_php::Framework;
use source_map_php::adapters::{detect_framework, extract_routes, extract_schema};
use source_map_php::composer::export_packages;
use source_map_php::config::IndexerConfig;
use source_map_php::extract::extract_symbols;
use source_map_php::sanitizer::Sanitizer;
use source_map_php::scanner::scan_repo;
use source_map_php::tests_linker::{extract_tests, link_symbols_and_routes};

fn fixture(name: &str) -> &'static Path {
    match name {
        "laravel" => Path::new("tests/fixtures/laravel"),
        "hyperf" => Path::new("tests/fixtures/hyperf"),
        _ => unreachable!(),
    }
}

#[test]
fn laravel_fixture_indexes_symbols_routes_tests_and_schema() {
    let repo = fixture("laravel");
    let config = IndexerConfig::default();
    let packages = export_packages(repo).unwrap();
    let framework = detect_framework(
        repo,
        Framework::Auto,
        &packages
            .packages
            .iter()
            .map(|package| package.name.clone())
            .collect::<Vec<_>>(),
    );
    let files = scan_repo(repo, &config.paths).unwrap();
    let mut symbols = extract_symbols(
        repo,
        &packages.root.name,
        framework,
        &files,
        &packages,
        &Sanitizer::default(),
    )
    .unwrap();
    let mut routes =
        extract_routes(repo, &packages.root.name, framework, &Sanitizer::default()).unwrap();
    let schema = extract_schema(repo, &packages.root.name).unwrap();
    let mut tests = extract_tests(repo, &packages.root.name, framework, &files).unwrap();
    link_symbols_and_routes(&mut symbols, &mut routes, &mut tests, &config.tests);

    assert_eq!(framework, Framework::Laravel);
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.fqn.ends_with("ConsentService::sign"))
    );
    assert!(
        routes
            .iter()
            .any(|route| route.uri == "/patients/{patient}/consents")
    );
    assert!(
        tests
            .iter()
            .any(|test| test.command.contains("artisan test"))
    );
    assert!(
        schema
            .iter()
            .any(|item| item.table.as_deref() == Some("consents"))
    );
}

#[test]
fn hyperf_fixture_indexes_config_and_attribute_routes() {
    let repo = fixture("hyperf");
    let config = IndexerConfig::default();
    let packages = export_packages(repo).unwrap();
    let framework = detect_framework(
        repo,
        Framework::Auto,
        &packages
            .packages
            .iter()
            .map(|package| package.name.clone())
            .collect::<Vec<_>>(),
    );
    let files = scan_repo(repo, &config.paths).unwrap();
    let mut symbols = extract_symbols(
        repo,
        &packages.root.name,
        framework,
        &files,
        &packages,
        &Sanitizer::default(),
    )
    .unwrap();
    let mut routes =
        extract_routes(repo, &packages.root.name, framework, &Sanitizer::default()).unwrap();
    let mut tests = extract_tests(repo, &packages.root.name, framework, &files).unwrap();
    link_symbols_and_routes(&mut symbols, &mut routes, &mut tests, &config.tests);

    assert_eq!(framework, Framework::Hyperf);
    assert!(routes.iter().any(|route| route.uri == "/consents"));
    assert!(tests.iter().any(|test| test.command.contains("co-phpunit")));
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.fqn.ends_with("ConsentController::store"))
    );
}

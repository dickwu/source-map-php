use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::{Framework, IndexMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDoc {
    pub id: String,
    pub stable_key: String,
    pub repo: String,
    pub framework: String,
    pub kind: String,
    pub short_name: String,
    pub fqn: String,
    pub owner_class: Option<String>,
    pub namespace: Option<String>,
    pub signature: Option<String>,
    pub doc_summary: Option<String>,
    pub doc_description: Option<String>,
    pub param_docs: Vec<String>,
    pub return_doc: Option<String>,
    pub throws_docs: Vec<String>,
    pub magic_methods: Vec<String>,
    pub magic_properties: Vec<String>,
    pub inline_rule_comments: Vec<String>,
    pub comment_keywords: Vec<String>,
    pub symbol_tokens: Vec<String>,
    pub framework_tags: Vec<String>,
    pub risk_tags: Vec<String>,
    pub route_ids: Vec<String>,
    pub related_symbols: Vec<String>,
    pub related_tests: Vec<String>,
    pub related_tests_count: u32,
    pub references_count: u32,
    pub validation_commands: Vec<String>,
    pub missing_test_warning: Option<String>,
    pub package_name: String,
    pub package_type: Option<String>,
    pub package_version: Option<String>,
    pub package_keywords: Vec<String>,
    pub is_vendor: bool,
    pub is_project_code: bool,
    pub is_test: bool,
    pub autoloadable: bool,
    pub extraction_confidence: String,
    pub path: String,
    pub absolute_path: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDoc {
    pub id: String,
    pub repo: String,
    pub framework: String,
    pub method: String,
    pub uri: String,
    pub route_name: Option<String>,
    pub action: Option<String>,
    pub controller: Option<String>,
    pub controller_method: Option<String>,
    pub middleware: Vec<String>,
    pub related_symbols: Vec<String>,
    pub related_tests: Vec<String>,
    pub package_name: String,
    pub path: Option<String>,
    pub line_start: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDoc {
    pub id: String,
    pub repo: String,
    pub framework: String,
    pub fqn: String,
    pub path: String,
    pub line_start: usize,
    pub covered_symbols: Vec<String>,
    pub referenced_symbols: Vec<String>,
    pub routes_called: Vec<String>,
    pub command: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDoc {
    pub id: String,
    pub repo: String,
    pub name: String,
    pub version: Option<String>,
    pub package_type: Option<String>,
    pub description: Option<String>,
    pub install_path: Option<String>,
    pub keywords: Vec<String>,
    pub is_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDoc {
    pub id: String,
    pub repo: String,
    pub migration: String,
    pub table: Option<String>,
    pub operation: String,
    pub path: String,
    pub line_start: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: String,
    pub repo_path: String,
    pub git_commit: String,
    pub composer_lock_hash: String,
    pub indexer_config_hash: String,
    pub framework: String,
    pub include_vendor: bool,
    pub include_tests: bool,
    pub mode: String,
    pub index_prefix: String,
    pub indexes: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit<T> {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(flatten)]
    pub document: T,
    #[serde(rename = "_rankingScore", default)]
    pub ranking_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse<T> {
    pub hits: Vec<SearchHit<T>>,
}

pub fn make_stable_id(parts: &[&str]) -> String {
    let raw = parts.join("|");
    let mut hasher = Sha1::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn manifest_path(repo: &Path, run_id: &str) -> std::path::PathBuf {
    repo.join("build")
        .join("index-runs")
        .join(format!("{run_id}.json"))
}

pub fn run_id(repo: &str, framework: Framework, mode: IndexMode) -> String {
    let raw = format!(
        "{}|{}|{}|{}",
        repo,
        framework.as_str(),
        mode.as_str(),
        Utc::now()
    );
    make_stable_id(&[&raw])
}

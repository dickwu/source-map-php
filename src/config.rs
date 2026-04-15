use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use url::Url;

use crate::Framework;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub meilisearch: MeilisearchConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub tests: TestsConfig,
    #[serde(default)]
    pub sanitizer: SanitizerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub slug: Option<String>,
    #[serde(default = "default_framework")]
    pub default_framework: Framework,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_allow_paths")]
    pub allow: Vec<String>,
    #[serde(default = "default_allow_vendor")]
    pub allow_vendor: bool,
    #[serde(default = "default_allow_vendor_paths")]
    pub allow_vendor_paths: Vec<String>,
    #[serde(default = "default_deny_paths")]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeilisearchConfig {
    #[serde(default = "default_meili_host")]
    pub host: String,
    #[serde(default = "default_index_prefix")]
    pub index_prefix: Option<String>,
    #[serde(default = "default_master_key_env")]
    pub master_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_exact_limit")]
    pub exact_limit: usize,
    #[serde(default = "default_natural_limit")]
    pub natural_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestsConfig {
    #[serde(default = "default_include_tests")]
    pub include_tests: bool,
    #[serde(default = "default_validate_threshold")]
    pub validate_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizerConfig {
    #[serde(default = "default_inline_comment_window")]
    pub inline_comment_window: usize,
}

#[derive(Debug, Clone)]
pub struct MeiliConnection {
    pub host: Url,
    pub api_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ConnectFile {
    host: Option<String>,
    api_key: Option<String>,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig {
                slug: None,
                default_framework: default_framework(),
                timezone: default_timezone(),
            },
            paths: PathsConfig {
                allow: default_allow_paths(),
                allow_vendor: default_allow_vendor(),
                allow_vendor_paths: default_allow_vendor_paths(),
                deny: default_deny_paths(),
            },
            meilisearch: MeilisearchConfig {
                host: default_meili_host(),
                index_prefix: default_index_prefix(),
                master_key_env: default_master_key_env(),
            },
            search: SearchConfig {
                exact_limit: default_exact_limit(),
                natural_limit: default_natural_limit(),
            },
            tests: TestsConfig {
                include_tests: default_include_tests(),
                validate_threshold: default_validate_threshold(),
            },
            sanitizer: SanitizerConfig {
                inline_comment_window: default_inline_comment_window(),
            },
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            slug: None,
            default_framework: default_framework(),
            timezone: default_timezone(),
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            allow: default_allow_paths(),
            allow_vendor: default_allow_vendor(),
            allow_vendor_paths: default_allow_vendor_paths(),
            deny: default_deny_paths(),
        }
    }
}

impl Default for MeilisearchConfig {
    fn default() -> Self {
        Self {
            host: default_meili_host(),
            index_prefix: default_index_prefix(),
            master_key_env: default_master_key_env(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            exact_limit: default_exact_limit(),
            natural_limit: default_natural_limit(),
        }
    }
}

impl Default for TestsConfig {
    fn default() -> Self {
        Self {
            include_tests: default_include_tests(),
            validate_threshold: default_validate_threshold(),
        }
    }
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            inline_comment_window: default_inline_comment_window(),
        }
    }
}

impl IndexerConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let config = toml::from_str::<Self>(&contents)
            .with_context(|| format!("parse {}", path.display()))?;
        Ok(config)
    }

    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn effective_index_prefix(&self, repo: &Path) -> String {
        if let Some(prefix) = &self.meilisearch.index_prefix {
            return prefix.clone();
        }
        if let Some(slug) = &self.project.slug {
            return slug.clone();
        }
        repo.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("source_map_php")
            .replace(
                |c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_',
                "-",
            )
    }

    pub fn resolve_meili(&self) -> Result<MeiliConnection> {
        let env_host = env::var("MEILI_HOST").ok();
        let env_api_key = env::var(&self.meilisearch.master_key_env).ok();
        let connect_path = default_connect_file_path();

        self.resolve_meili_with_sources(&connect_path, env_host.as_deref(), env_api_key.as_deref())
    }

    fn resolve_meili_with_sources(
        &self,
        connect_path: &Path,
        env_host: Option<&str>,
        env_api_key: Option<&str>,
    ) -> Result<MeiliConnection> {
        let connect_file = ConnectFile::load(connect_path)?;

        let host_source = env_host
            .map(ToOwned::to_owned)
            .or_else(|| {
                if self.meilisearch.host != default_meili_host() {
                    Some(self.meilisearch.host.clone())
                } else {
                    None
                }
            })
            .or(connect_file.host)
            .unwrap_or_else(|| self.meilisearch.host.clone());

        let host = Url::parse(&host_source)
            .with_context(|| format!("invalid MEILI_HOST {host_source}"))?;
        let api_key = env_api_key
            .map(ToOwned::to_owned)
            .or(connect_file.api_key)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing meilisearch api key in env {} or {}",
                    self.meilisearch.master_key_env,
                    connect_path.display()
                )
            })?;
        Ok(MeiliConnection { host, api_key })
    }

    pub fn hash(&self) -> Result<String> {
        let raw = self.to_toml_string()?;
        let mut hasher = Sha1::new();
        hasher.update(raw.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }
}

impl ConnectFile {
    fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::from_json(&raw).with_context(|| format!("parse {}", path.display()))
    }

    fn from_json(raw: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(raw)?;
        Ok(Self {
            host: value_lookup(&value, &["host", "url", "endpoint"]).or_else(|| {
                nested_lookup(
                    &value,
                    &["connection", "default", "meilisearch"],
                    &["host", "url", "endpoint"],
                )
            }),
            api_key: value_lookup(
                &value,
                &["api_key", "apiKey", "master_key", "masterKey", "key"],
            )
            .or_else(|| {
                nested_lookup(
                    &value,
                    &["connection", "default", "meilisearch"],
                    &["api_key", "apiKey", "master_key", "masterKey", "key"],
                )
            }),
        })
    }
}

fn value_lookup(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|item| item.to_string())
    })
}

fn nested_lookup(value: &Value, containers: &[&str], keys: &[&str]) -> Option<String> {
    containers.iter().find_map(|container| {
        value
            .get(container)
            .and_then(|nested| value_lookup(nested, keys))
    })
}

pub(crate) fn default_connect_file_path() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".config/meilisearch/connect.json")
}

fn default_framework() -> Framework {
    Framework::Auto
}

fn default_timezone() -> String {
    "America/Winnipeg".to_string()
}

fn default_allow_paths() -> Vec<String> {
    vec![
        "app".into(),
        "src".into(),
        "routes".into(),
        "config".into(),
        "database/migrations".into(),
        "database/factories".into(),
        "database/seeders".into(),
        "tests".into(),
        "test".into(),
        "composer.json".into(),
        "composer.lock".into(),
        "phpunit.xml".into(),
        "pest.php".into(),
    ]
}

fn default_allow_vendor() -> bool {
    true
}

fn default_allow_vendor_paths() -> Vec<String> {
    vec!["vendor/*/*/src".into(), "vendor/*/*/composer.json".into()]
}

fn default_deny_paths() -> Vec<String> {
    vec![
        ".env".into(),
        ".env.*".into(),
        "storage".into(),
        "bootstrap/cache".into(),
        "public/storage".into(),
        "var/log".into(),
        "logs".into(),
        "tmp".into(),
        "dump".into(),
        "dumps".into(),
        "backups".into(),
        "*.sql".into(),
        "*.sqlite".into(),
        "*.db".into(),
        "*.dump".into(),
        "*.bak".into(),
        "*.csv".into(),
        "*.xlsx".into(),
        "*.xls".into(),
        "*.parquet".into(),
        "*.zip".into(),
        "*.tar".into(),
        "*.gz".into(),
        "*.7z".into(),
        "node_modules".into(),
    ]
}

fn default_meili_host() -> String {
    "http://127.0.0.1:7700".to_string()
}

fn default_index_prefix() -> Option<String> {
    None
}

fn default_master_key_env() -> String {
    "MEILI_MASTER_KEY".to_string()
}

fn default_exact_limit() -> usize {
    8
}

fn default_natural_limit() -> usize {
    10
}

fn default_include_tests() -> bool {
    true
}

fn default_validate_threshold() -> f32 {
    0.60
}

fn default_inline_comment_window() -> usize {
    2
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::{ConnectFile, IndexerConfig};

    #[test]
    fn defaults_derive_prefix_from_repo_name() {
        let config = IndexerConfig::default();
        let dir = tempdir().unwrap();
        let repo = dir.path().join("my-php-repo");
        std::fs::create_dir_all(&repo).unwrap();

        assert_eq!(config.effective_index_prefix(&repo), "my-php-repo");
    }

    #[test]
    fn loads_config_from_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        std::fs::write(
            &path,
            r#"[project]
slug = "custom"
"#,
        )
        .unwrap();

        let config = IndexerConfig::load(&path).unwrap();
        assert_eq!(config.project.slug.as_deref(), Some("custom"));
    }

    #[test]
    fn parses_flat_connect_file_shape() {
        let raw = r#"{
          "host": "http://meili.example:7700",
          "api_key": "secret"
        }"#;

        let parsed = ConnectFile::from_json(raw).unwrap();
        assert_eq!(parsed.host.as_deref(), Some("http://meili.example:7700"));
        assert_eq!(parsed.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn parses_nested_connect_file_shape() {
        let raw = r#"{
          "connection": {
            "url": "http://nested.example:7700",
            "masterKey": "nested-secret"
          }
        }"#;

        let parsed = ConnectFile::from_json(raw).unwrap();
        assert_eq!(parsed.host.as_deref(), Some("http://nested.example:7700"));
        assert_eq!(parsed.api_key.as_deref(), Some("nested-secret"));
    }

    #[test]
    fn connect_file_fills_missing_api_key() {
        let dir = tempdir().unwrap();
        let connect_path = dir.path().join("connect.json");
        std::fs::write(
            &connect_path,
            r#"{"url":"http://file.example:7700","apiKey":"from-file"}"#,
        )
        .unwrap();

        let config = IndexerConfig::default();
        let connection = config
            .resolve_meili_with_sources(&connect_path, None, None)
            .unwrap();

        assert_eq!(connection.host.as_str(), "http://file.example:7700/");
        assert_eq!(connection.api_key, "from-file");
    }

    #[test]
    fn explicit_config_host_beats_connect_file_host() {
        let dir = tempdir().unwrap();
        let connect_path = dir.path().join("connect.json");
        std::fs::write(
            &connect_path,
            r#"{"url":"http://file.example:7700","apiKey":"from-file"}"#,
        )
        .unwrap();

        let mut config = IndexerConfig::default();
        config.meilisearch.host = "http://project.example:7700".to_string();

        let connection = config
            .resolve_meili_with_sources(&connect_path, None, None)
            .unwrap();

        assert_eq!(connection.host.as_str(), "http://project.example:7700/");
        assert_eq!(connection.api_key, "from-file");
    }

    #[test]
    fn env_values_beat_connect_file() {
        let dir = tempdir().unwrap();
        let connect_path = dir.path().join("connect.json");
        std::fs::write(
            &connect_path,
            r#"{"url":"http://file.example:7700","apiKey":"from-file"}"#,
        )
        .unwrap();

        let config = IndexerConfig::default();
        let connection = config
            .resolve_meili_with_sources(
                &connect_path,
                Some("http://env.example:7700"),
                Some("from-env"),
            )
            .unwrap();

        assert_eq!(connection.host.as_str(), "http://env.example:7700/");
        assert_eq!(connection.api_key, "from-env");
    }

    #[test]
    fn missing_sources_still_errors_for_api_key() {
        let config = IndexerConfig::default();
        let err = config
            .resolve_meili_with_sources(Path::new("/definitely/missing/connect.json"), None, None)
            .unwrap_err();

        assert!(err.to_string().contains("missing meilisearch api key"));
    }
}

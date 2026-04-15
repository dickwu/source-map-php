use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub name: String,
    pub repo_path: String,
    pub index_prefix: String,
    pub framework: String,
    pub meili_host: String,
    pub last_run_id: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRegistry {
    #[serde(default)]
    pub projects: Vec<ProjectRecord>,
}

impl ProjectRegistry {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent directory for {}", path.display()))?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    pub fn upsert(&mut self, record: ProjectRecord) {
        if let Some(existing) = self
            .projects
            .iter_mut()
            .find(|item| item.repo_path == record.repo_path || item.name == record.name)
        {
            *existing = record;
        } else {
            self.projects.push(record);
        }
        self.projects
            .sort_by(|left, right| left.name.cmp(&right.name).then(left.repo_path.cmp(&right.repo_path)));
    }

    pub fn resolve<'a>(&'a self, selector: &str) -> Option<&'a ProjectRecord> {
        let selector_path = canonicalized(selector);
        self.projects.iter().find(|record| {
            record.name == selector
                || record.repo_path == selector
                || selector_path
                    .as_deref()
                    .is_some_and(|path| path == record.repo_path)
        })
    }

    pub fn remove(&mut self, selector: &str) -> Option<ProjectRecord> {
        let selector_path = canonicalized(selector);
        let index = self.projects.iter().position(|record| {
            record.name == selector
                || record.repo_path == selector
                || selector_path
                    .as_deref()
                    .is_some_and(|path| path == record.repo_path)
        })?;
        Some(self.projects.remove(index))
    }
}

pub fn default_project_registry_path() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".config/meilisearch/project.json")
}

fn canonicalized(value: &str) -> Option<String> {
    fs::canonicalize(value)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{ProjectRecord, ProjectRegistry};
    use chrono::Utc;

    fn record(name: &str, repo_path: &str) -> ProjectRecord {
        ProjectRecord {
            name: name.to_string(),
            repo_path: repo_path.to_string(),
            index_prefix: name.to_string(),
            framework: "hyperf".to_string(),
            meili_host: "http://127.0.0.1:7700".to_string(),
            last_run_id: "run".to_string(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn resolves_by_name_or_path() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("staff-api");
        std::fs::create_dir_all(&repo).unwrap();

        let mut registry = ProjectRegistry::default();
        registry.upsert(record("staff-api", &repo.to_string_lossy()));

        assert!(registry.resolve("staff-api").is_some());
        assert!(registry.resolve(&repo.to_string_lossy()).is_some());
    }

    #[test]
    fn upsert_replaces_existing_repo_entry() {
        let mut registry = ProjectRegistry::default();
        registry.upsert(record("staff-api", "/tmp/staff-api"));
        let mut updated = record("staff-api-prod", "/tmp/staff-api");
        updated.index_prefix = "staff-api-prod".to_string();
        registry.upsert(updated.clone());

        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].name, "staff-api-prod");
        assert_eq!(registry.projects[0].index_prefix, "staff-api-prod");
    }

    #[test]
    fn remove_deletes_matching_record() {
        let mut registry = ProjectRegistry::default();
        registry.upsert(record("staff-api", "/tmp/staff-api"));
        registry.upsert(record("front-api", "/tmp/front-api"));

        let removed = registry.remove("staff-api").unwrap();

        assert_eq!(removed.name, "staff-api");
        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].name, "front-api");
    }
}

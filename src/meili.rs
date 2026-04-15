use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::MeiliConnection;
use crate::models::SearchResponse;

#[derive(Debug, Clone)]
pub struct MeiliClient {
    client: Client,
    connection: MeiliConnection,
}

impl MeiliClient {
    pub fn new(connection: MeiliConnection) -> Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            connection,
        })
    }

    pub fn health(&self) -> Result<Value> {
        self.get("health")
    }

    pub fn create_index(&self, name: &str) -> Result<()> {
        let response = self
            .client
            .post(self.url("indexes")?)
            .bearer_auth(&self.connection.api_key)
            .json(&json!({ "uid": name }))
            .send()?;
        if response.status().is_success() || response.status().as_u16() == 409 {
            return Ok(());
        }
        Err(anyhow!(
            "failed to create index {name}: {}",
            response.text()?
        ))
    }

    pub fn apply_settings(&self, index: &str, settings: &Value) -> Result<()> {
        let task = self
            .client
            .patch(self.url(&format!("indexes/{index}/settings"))?)
            .bearer_auth(&self.connection.api_key)
            .json(settings)
            .send()?
            .json::<Value>()?;
        self.wait_for_task(task_uid(&task)?)?;
        Ok(())
    }

    pub fn replace_documents<T: Serialize>(&self, index: &str, documents: &[T]) -> Result<()> {
        let task = self
            .client
            .post(self.url(&format!("indexes/{index}/documents"))?)
            .bearer_auth(&self.connection.api_key)
            .json(documents)
            .send()?
            .json::<Value>()?;
        self.wait_for_task(task_uid(&task)?)?;
        Ok(())
    }

    pub fn search<T: serde::de::DeserializeOwned>(
        &self,
        index: &str,
        body: Value,
    ) -> Result<SearchResponse<T>> {
        Ok(self
            .client
            .post(self.url(&format!("indexes/{index}/search"))?)
            .bearer_auth(&self.connection.api_key)
            .json(&body)
            .send()?
            .json()?)
    }

    pub fn stats(&self, index: &str) -> Result<Value> {
        self.get(&format!("indexes/{index}/stats"))
    }

    pub fn swap_indexes(&self, swaps: Vec<(String, String)>) -> Result<()> {
        let payload = swaps
            .into_iter()
            .map(|(indexes_a, indexes_b)| json!({ "indexes": [indexes_a, indexes_b] }))
            .collect::<Vec<_>>();
        let task = self
            .client
            .post(self.url("swap-indexes")?)
            .bearer_auth(&self.connection.api_key)
            .json(&payload)
            .send()?
            .json::<Value>()?;
        self.wait_for_task(task_uid(&task)?)?;
        Ok(())
    }

    pub fn wait_for_task(&self, uid: u64) -> Result<()> {
        for _ in 0..50 {
            let task = self.get(&format!("tasks/{uid}"))?;
            match task.get("status").and_then(Value::as_str) {
                Some("succeeded") => return Ok(()),
                Some("failed") => return Err(anyhow!("meilisearch task {uid} failed: {task}")),
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        Err(anyhow!("timed out waiting for meilisearch task {uid}"))
    }

    fn get(&self, path: &str) -> Result<Value> {
        Ok(self
            .client
            .get(self.url(path)?)
            .bearer_auth(&self.connection.api_key)
            .send()?
            .json()?)
    }

    fn url(&self, path: &str) -> Result<reqwest::Url> {
        self.connection
            .host
            .join(path)
            .with_context(|| format!("join meilisearch path {path}"))
    }
}

pub fn symbols_settings() -> Value {
    json!({
        "searchableAttributes": [
            "short_name", "fqn", "owner_class", "namespace", "symbol_tokens", "signature",
            "doc_summary", "doc_description", "param_docs", "return_doc", "throws_docs",
            "inline_rule_comments", "comment_keywords", "framework_tags", "package_name", "path"
        ],
        "filterableAttributes": [
            "repo", "framework", "kind", "package_name", "is_vendor", "is_project_code", "is_test", "route_ids", "risk_tags"
        ],
        "sortableAttributes": ["is_project_code", "related_tests_count", "references_count", "line_start"],
        "displayedAttributes": [
            "id", "stable_key", "kind", "fqn", "signature", "doc_summary", "path", "line_start", "package_name", "related_tests", "missing_test_warning"
        ]
    })
}

pub fn routes_settings() -> Value {
    json!({
        "searchableAttributes": ["uri", "route_name", "action", "controller", "controller_method", "middleware"],
        "filterableAttributes": ["repo", "framework", "method"],
    })
}

pub fn tests_settings() -> Value {
    json!({
        "searchableAttributes": ["fqn", "covered_symbols", "referenced_symbols", "routes_called", "command"],
        "filterableAttributes": ["repo", "framework"],
    })
}

pub fn packages_settings() -> Value {
    json!({
        "searchableAttributes": ["name", "description", "keywords"],
        "filterableAttributes": ["repo", "type"]
    })
}

pub fn schema_settings() -> Value {
    json!({
        "searchableAttributes": ["migration", "table", "operation", "path"],
        "filterableAttributes": ["repo", "operation"]
    })
}

pub fn runs_settings() -> Value {
    json!({
        "searchableAttributes": ["run_id", "framework", "mode"],
        "filterableAttributes": ["framework", "mode", "index_prefix"]
    })
}

fn task_uid(value: &Value) -> Result<u64> {
    value
        .get("taskUid")
        .or_else(|| value.get("uid"))
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("meilisearch response missing task uid: {value}"))
}

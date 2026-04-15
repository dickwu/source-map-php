use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use url::Url;

use super::DeclarationCandidate;

const MAX_REFERENCE_LOOKUPS_TOTAL: usize = 64;
const MAX_REFERENCE_LOOKUPS_PER_FILE: usize = 4;

pub struct PhpactorExtractor {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    initialized: bool,
    root_uri: Url,
    remaining_reference_lookups: usize,
}

impl PhpactorExtractor {
    pub fn connect(repo: &Path) -> Result<Self> {
        let mut child = spawn_lsp_process(repo)?;
        let stdin = child.stdin.take().context("capture lsp stdin")?;
        let stdout = child.stdout.take().context("capture lsp stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            initialized: false,
            root_uri: Url::from_directory_path(repo).map_err(|_| anyhow!("invalid repo path"))?,
            remaining_reference_lookups: MAX_REFERENCE_LOOKUPS_TOTAL,
        })
    }

    pub fn extract_candidates(
        &mut self,
        file: &Path,
        contents: &str,
    ) -> Result<Vec<DeclarationCandidate>> {
        self.ensure_initialized()?;
        let uri = Url::from_file_path(file).map_err(|_| anyhow!("invalid file path"))?;
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "php",
                    "version": 1,
                    "text": contents,
                }
            }),
        )?;
        let result = self.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        let mut declarations = parse_document_symbols(&result);

        for declaration in declarations
            .iter_mut()
            .filter(|item| is_important_symbol(item))
            .take(MAX_REFERENCE_LOOKUPS_PER_FILE)
        {
            if self.remaining_reference_lookups == 0 {
                break;
            }
            self.remaining_reference_lookups -= 1;
            let position = json!({
                "textDocument": { "uri": uri },
                "position": { "line": declaration.line_start.saturating_sub(1), "character": 0 }
            });
            let _ = self.request("textDocument/definition", position.clone());
            if let Ok(references) = self.request(
                "textDocument/references",
                json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": declaration.line_start.saturating_sub(1), "character": 0 },
                    "context": { "includeDeclaration": true }
                }),
            ) {
                declaration.references_count = references
                    .as_array()
                    .map(|items| items.len() as u32)
                    .unwrap_or(0);
                declaration.extraction_confidence = "phpantom_lsp".to_string();
            }
        }

        Ok(declarations)
    }

    fn ensure_initialized(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }
        let _ = self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": self.root_uri,
                "capabilities": {},
                "clientInfo": { "name": "source-map-php" },
            }),
        )?;
        self.notify("initialized", json!({}))?;
        self.initialized = true;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        loop {
            let message = self.read_message()?;
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(anyhow!("lsp {method} failed: {error}"));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn send(&mut self, value: Value) -> Result<()> {
        let raw = serde_json::to_vec(&value)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", raw.len())?;
        self.stdin.write_all(&raw)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_message(&mut self) -> Result<Value> {
        let mut content_length = None::<usize>;
        loop {
            let mut line = String::new();
            let bytes = self.stdout.read_line(&mut line)?;
            if bytes == 0 {
                return Err(anyhow!("lsp closed stdout"));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse()?);
            }
        }
        let length = content_length.ok_or_else(|| anyhow!("missing content length from lsp"))?;
        let mut buf = vec![0u8; length];
        self.stdout.read_exact(&mut buf)?;
        Ok(serde_json::from_slice(&buf)?)
    }
}

impl Drop for PhpactorExtractor {
    fn drop(&mut self) {
        if self.initialized {
            let _ = self.request("shutdown", json!(null));
            let _ = self.notify("exit", json!(null));
        }
        let _ = self.child.kill();
    }
}

fn spawn_lsp_process(repo: &Path) -> Result<Child> {
    let candidates = [
        embedded_phpantom_path(),
        Some(PathBuf::from("phpantom_lsp")),
        Some(PathBuf::from("phpactor")),
    ];

    for candidate in candidates.into_iter().flatten() {
        let mut command = Command::new(&candidate);
        if candidate.file_name().and_then(|name| name.to_str()) == Some("phpactor") {
            command.arg("language-server");
        }
        let child = command
            .current_dir(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = child {
            return Ok(child);
        }
    }

    Err(anyhow!(
        "failed to spawn phpantom_lsp or phpactor language server"
    ))
}

fn embedded_phpantom_path() -> Option<PathBuf> {
    let current = env::current_exe().ok()?;
    let sibling = current.parent()?.join("phpantom_lsp");
    if sibling.exists() {
        Some(sibling)
    } else {
        None
    }
}

fn is_important_symbol(item: &DeclarationCandidate) -> bool {
    matches!(
        item.kind.as_str(),
        "class" | "interface" | "trait" | "enum" | "function"
    )
}

fn parse_document_symbols(value: &Value) -> Vec<DeclarationCandidate> {
    fn walk(
        node: &Value,
        namespace: Option<String>,
        owner_class: Option<String>,
        out: &mut Vec<DeclarationCandidate>,
    ) {
        if let Some(kind) = node.get("kind").and_then(Value::as_u64) {
            let name = node
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let range = node
                .get("range")
                .and_then(|value| value.get("start"))
                .and_then(|value| value.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize
                + 1;
            let end = node
                .get("range")
                .and_then(|value| value.get("end"))
                .and_then(|value| value.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize
                + 1;
            let mapped_kind = match kind {
                5 => "class",
                11 => "interface",
                12 => "function",
                6 => "method",
                23 => "enum",
                _ => "symbol",
            }
            .to_string();

            let next_owner =
                if mapped_kind == "class" || mapped_kind == "interface" || mapped_kind == "enum" {
                    Some(name.clone())
                } else {
                    owner_class.clone()
                };

            out.push(DeclarationCandidate {
                kind: mapped_kind.clone(),
                name: name.clone(),
                owner_class: owner_class.clone().filter(|_| mapped_kind == "method"),
                namespace: namespace.clone(),
                line_start: range,
                line_end: end,
                signature: node
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                extraction_confidence: "phpantom_lsp".to_string(),
                references_count: 0,
            });

            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for child in children {
                    walk(child, namespace.clone(), next_owner.clone(), out);
                }
            }
        }
    }

    let mut out = Vec::new();
    if let Some(array) = value.as_array() {
        for node in array {
            walk(node, None, None, &mut out);
        }
    }
    out
}

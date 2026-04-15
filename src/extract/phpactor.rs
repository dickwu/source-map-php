use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use url::Url;

use super::DeclarationCandidate;

pub struct PhpactorExtractor {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    initialized: bool,
    root_uri: Url,
}

impl PhpactorExtractor {
    pub fn connect(repo: &Path) -> Result<Self> {
        let mut child = Command::new("phpactor")
            .arg("language-server")
            .current_dir(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn phpactor language-server")?;
        let stdin = child.stdin.take().context("capture phpactor stdin")?;
        let stdout = child.stdout.take().context("capture phpactor stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            initialized: false,
            root_uri: Url::from_directory_path(repo).map_err(|_| anyhow!("invalid repo path"))?,
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
        parse_document_symbols(&result)
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
                    return Err(anyhow!("phpactor {method} failed: {error}"));
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
                return Err(anyhow!("phpactor closed stdout"));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse()?);
            }
        }
        let length =
            content_length.ok_or_else(|| anyhow!("missing content length from phpactor"))?;
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

fn parse_document_symbols(value: &Value) -> Result<Vec<DeclarationCandidate>> {
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
                extraction_confidence: "phpactor".to_string(),
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
    Ok(out)
}

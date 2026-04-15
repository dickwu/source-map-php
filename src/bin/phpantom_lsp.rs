use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use url::Url;

use source_map_php::extract::{DeclarationCandidate, fallback_candidates};

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = stdout.lock();

    let mut state = ServerState::default();

    loop {
        let message = read_message(&mut input)?;
        let method = message.get("method").and_then(Value::as_str);
        match method {
            Some("initialize") => {
                if let Some(root_uri) = message
                    .get("params")
                    .and_then(|params| params.get("rootUri"))
                    .and_then(Value::as_str)
                {
                    state.root = Url::parse(root_uri)
                        .ok()
                        .and_then(|url| url.to_file_path().ok());
                }
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 1,
                            "documentSymbolProvider": true,
                            "definitionProvider": true,
                            "referencesProvider": true
                        }
                    }
                });
                write_message(&mut output, &response)?;
            }
            Some("initialized") => {}
            Some("shutdown") => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": Value::Null
                });
                write_message(&mut output, &response)?;
            }
            Some("exit") => break,
            Some("textDocument/didOpen") => {
                if let Some(text_document) = message
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                {
                    if let (Some(uri), Some(text)) = (
                        text_document.get("uri").and_then(Value::as_str),
                        text_document.get("text").and_then(Value::as_str),
                    ) {
                        state.docs.insert(uri.to_string(), text.to_string());
                    }
                }
            }
            Some("textDocument/documentSymbol") => {
                let uri = doc_uri(&message)?;
                let text = state.docs.get(&uri).cloned().unwrap_or_default();
                let declarations = fallback_candidates(&text);
                let result = document_symbols(&declarations);
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": result
                });
                write_message(&mut output, &response)?;
            }
            Some("textDocument/definition") => {
                let uri = doc_uri(&message)?;
                let text = state.docs.get(&uri).cloned().unwrap_or_default();
                let declarations = fallback_candidates(&text);
                let line = message
                    .get("params")
                    .and_then(|params| params.get("position"))
                    .and_then(|position| position.get("line"))
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as usize
                    + 1;
                let result = declarations
                    .iter()
                    .find(|candidate| candidate.line_start == line)
                    .map(|candidate| location(&uri, candidate))
                    .unwrap_or(Value::Null);
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": result
                });
                write_message(&mut output, &response)?;
            }
            Some("textDocument/references") => {
                let uri = doc_uri(&message)?;
                let text = state.docs.get(&uri).cloned().unwrap_or_default();
                let declarations = fallback_candidates(&text);
                let line = message
                    .get("params")
                    .and_then(|params| params.get("position"))
                    .and_then(|position| position.get("line"))
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as usize
                    + 1;
                let result = if let Some(candidate) = declarations
                    .iter()
                    .find(|candidate| candidate.line_start == line)
                {
                    state.workspace_references(candidate)?
                } else {
                    Vec::new()
                };
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": result
                });
                write_message(&mut output, &response)?;
            }
            Some(_) | None => {
                if message.get("id").is_some() {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": message.get("id").cloned().unwrap_or(Value::Null),
                        "result": Value::Null
                    });
                    write_message(&mut output, &response)?;
                }
            }
        }
    }

    Ok(())
}

#[derive(Default)]
struct ServerState {
    root: Option<PathBuf>,
    docs: HashMap<String, String>,
    workspace_files: Option<Vec<(String, String)>>,
}

impl ServerState {
    fn workspace_references(
        &mut self,
        candidate: &DeclarationCandidate,
    ) -> anyhow::Result<Vec<Value>> {
        let Some(root) = &self.root else {
            return Ok(Vec::new());
        };
        let needle = &candidate.name;
        let mut out = Vec::new();
        if self.workspace_files.is_none() {
            let cached = php_files(root)
                .into_iter()
                .map(|path| {
                    let uri = Url::from_file_path(&path)
                        .ok()
                        .map(|url| url.to_string())
                        .unwrap_or_default();
                    let contents = fs::read_to_string(&path).unwrap_or_default();
                    (uri, contents)
                })
                .collect();
            self.workspace_files = Some(cached);
        }
        for (uri, contents) in self.workspace_files.as_ref().unwrap() {
            for (idx, line) in contents.lines().enumerate() {
                if line.contains(needle) {
                    out.push(json!({
                        "uri": uri,
                        "range": {
                            "start": { "line": idx, "character": 0 },
                            "end": { "line": idx, "character": line.len() }
                        }
                    }));
                }
            }
        }
        Ok(out)
    }
}

fn php_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("php"))
        .filter(|entry| {
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let rel = rel.to_string_lossy();
            !rel.starts_with(".git/")
                && !rel.starts_with("vendor/")
                && !rel.starts_with("node_modules/")
                && !rel.contains("/.")
        })
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

fn document_symbols(declarations: &[DeclarationCandidate]) -> Value {
    let mut classes: HashMap<String, Value> = HashMap::new();
    let mut tops = Vec::new();

    for declaration in declarations {
        let kind = symbol_kind(&declaration.kind);
        let symbol = json!({
            "name": declaration.name,
            "kind": kind,
            "detail": declaration.signature,
            "range": range(declaration),
            "selectionRange": range(declaration),
            "children": []
        });
        if declaration.kind == "method" {
            if let Some(owner) = &declaration.owner_class {
                classes
                    .entry(owner.clone())
                    .or_insert_with(|| {
                        json!({
                            "name": owner,
                            "kind": 5,
                            "detail": Value::Null,
                            "range": range(declaration),
                            "selectionRange": range(declaration),
                            "children": []
                        })
                    })
                    .get_mut("children")
                    .and_then(Value::as_array_mut)
                    .unwrap()
                    .push(symbol);
            } else {
                tops.push(symbol);
            }
        } else if matches!(
            declaration.kind.as_str(),
            "class" | "interface" | "enum" | "trait"
        ) {
            classes.insert(declaration.name.clone(), symbol);
        } else {
            tops.push(symbol);
        }
    }

    tops.extend(classes.into_values());
    Value::Array(tops)
}

fn location(uri: &str, candidate: &DeclarationCandidate) -> Value {
    json!({
        "uri": uri,
        "range": range(candidate)
    })
}

fn range(candidate: &DeclarationCandidate) -> Value {
    json!({
        "start": { "line": candidate.line_start.saturating_sub(1), "character": 0 },
        "end": { "line": candidate.line_end.saturating_sub(1), "character": 0 }
    })
}

fn symbol_kind(kind: &str) -> u64 {
    match kind {
        "class" => 5,
        "method" => 6,
        "function" => 12,
        "interface" => 11,
        "enum" => 23,
        "trait" => 5,
        _ => 13,
    }
}

fn doc_uri(message: &Value) -> anyhow::Result<String> {
    message
        .get("params")
        .and_then(|params| params.get("textDocument"))
        .and_then(|doc| doc.get("uri"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("missing textDocument.uri"))
}

fn read_message(input: &mut BufReader<impl Read>) -> anyhow::Result<Value> {
    let mut content_length = None::<usize>;
    loop {
        let mut line = String::new();
        let bytes = input.read_line(&mut line)?;
        if bytes == 0 {
            return Err(anyhow::anyhow!("stdin closed"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse()?);
        }
    }
    let length = content_length.ok_or_else(|| anyhow::anyhow!("missing content length"))?;
    let mut buf = vec![0u8; length];
    input.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

fn write_message(output: &mut impl Write, value: &Value) -> anyhow::Result<()> {
    let raw = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", raw.len())?;
    output.write_all(&raw)?;
    output.flush()?;
    Ok(())
}

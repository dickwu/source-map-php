use regex::Regex;

use super::DeclarationCandidate;

pub fn extract_candidates(contents: &str) -> Vec<DeclarationCandidate> {
    let namespace_re = Regex::new(r"^\s*namespace\s+([^;]+);").unwrap();
    let class_re = Regex::new(
        r"^\s*(?:final\s+|abstract\s+)?(class|interface|trait|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap();
    let function_re = Regex::new(
        r"^\s*(?:(public|protected|private)\s+)?(?:(static)\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)",
    )
    .unwrap();

    let mut namespace = None;
    let mut current_class = None::<String>;
    let mut class_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut out = Vec::new();

    for (idx, line) in contents.lines().enumerate() {
        if let Some(caps) = namespace_re.captures(line) {
            namespace = caps.get(1).map(|item| item.as_str().trim().to_string());
        }
        if let Some(caps) = class_re.captures(line) {
            let name = caps.get(2).unwrap().as_str().to_string();
            let kind = caps.get(1).unwrap().as_str().to_string();
            out.push(DeclarationCandidate {
                kind,
                name: name.clone(),
                owner_class: None,
                namespace: namespace.clone(),
                line_start: idx + 1,
                line_end: idx + 1,
                signature: Some(line.trim().to_string()),
                extraction_confidence: "fallback".to_string(),
            });
            current_class = Some(name);
            class_depth = brace_depth + line.matches('{').count();
        } else if let Some(caps) = function_re.captures(line) {
            let name = caps.get(3).unwrap().as_str().to_string();
            let owner_class = current_class.clone();
            out.push(DeclarationCandidate {
                kind: if owner_class.is_some() {
                    "method".to_string()
                } else {
                    "function".to_string()
                },
                name,
                owner_class,
                namespace: namespace.clone(),
                line_start: idx + 1,
                line_end: idx + 1,
                signature: Some(line.trim().to_string()),
                extraction_confidence: "fallback".to_string(),
            });
        }

        brace_depth += line.matches('{').count();
        brace_depth = brace_depth.saturating_sub(line.matches('}').count());
        if current_class.is_some() && brace_depth < class_depth {
            current_class = None;
            class_depth = 0;
        }
    }

    out
}

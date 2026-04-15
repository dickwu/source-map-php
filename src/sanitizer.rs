use regex::Regex;

#[derive(Debug, Clone)]
pub struct Sanitizer {
    secret_patterns: Vec<Regex>,
    phi_patterns: Vec<Regex>,
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self {
            secret_patterns: vec![
                Regex::new(r"-----BEGIN (RSA|EC|OPENSSH|PRIVATE) KEY-----").unwrap(),
                Regex::new(r"(?i)\b(password|passwd|secret|token|api[-_]?key)\b\s*[:=]").unwrap(),
                Regex::new(r"\beyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9._-]+\.[a-zA-Z0-9._-]+\b").unwrap(),
                Regex::new(r"(?i)\b(mysql|pgsql|postgres|redis|mongodb)://\S+").unwrap(),
            ],
            phi_patterns: vec![
                Regex::new(r"(?i)\b\d{3}-\d{2}-\d{4}\b").unwrap(),
                Regex::new(r"(?i)\b(?:mrn|medical record number|health card)\b").unwrap(),
                Regex::new(r"\b\d{10,}\b").unwrap(),
                Regex::new(r"(?i)\b\d{4}-\d{2}-\d{2}\b").unwrap(),
                Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap(),
                Regex::new(r"(?i)\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b").unwrap(),
            ],
        }
    }
}

impl Sanitizer {
    pub fn sanitize_text(&self, input: &str) -> Option<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        if self
            .secret_patterns
            .iter()
            .chain(self.phi_patterns.iter())
            .any(|pattern| pattern.is_match(trimmed))
        {
            return None;
        }
        Some(trimmed.to_string())
    }

    pub fn sanitize_many<'a>(&self, values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        values
            .into_iter()
            .filter_map(|value| self.sanitize_text(value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Sanitizer;

    #[test]
    fn drops_secrets_and_phi() {
        let sanitizer = Sanitizer::default();
        assert!(sanitizer.sanitize_text("password=secret").is_none());
        assert!(sanitizer.sanitize_text("patient@example.com").is_none());
        assert!(sanitizer.sanitize_text("MRN 1234567890").is_none());
    }

    #[test]
    fn keeps_safe_rule_comments() {
        let sanitizer = Sanitizer::default();
        assert_eq!(
            sanitizer
                .sanitize_text("Consent becomes immutable after signing.")
                .as_deref(),
            Some("Consent becomes immutable after signing.")
        );
    }
}

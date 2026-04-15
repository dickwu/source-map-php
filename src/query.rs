use std::collections::BTreeSet;

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "where", "what", "how", "for", "to", "before", "after", "and",
    "or", "in", "of", "with", "on",
];

pub fn compact_query(input: &str) -> String {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();

    for token in input
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '\\')
        .filter(|token| !token.is_empty())
    {
        let lower = token.to_ascii_lowercase();
        if STOP_WORDS.contains(&lower.as_str()) {
            continue;
        }
        if seen.insert(lower) {
            result.push(token.to_string());
        }
        if result.len() >= 10 {
            break;
        }
    }

    result.join(" ")
}

#[cfg(test)]
mod tests {
    use super::compact_query;

    #[test]
    fn strips_noise_and_limits_words() {
        let query = compact_query(
            "where is consent checked before discharge export in the patient service and controller",
        );
        assert_eq!(
            query,
            "consent checked discharge export patient service controller"
        );
    }
}

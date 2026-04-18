//! fog-memory/compressor.rs — Token estimation and context compression
//!
//! Lightweight token budget enforcement for route_map and export_snapshot.
//! No external tokenizer dependency — uses character-based approximation.
//!
//! Rule: 1 token ≈ 4 chars (English prose/code average)
//!
//! PATTERN_DECISION: Level 1 (Pure Functions)
//! No side effects. All functions: (&str | usize) → usize.

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the number of tokens in a string.
///
/// Uses the 4-chars-per-token heuristic (standard for GPT-class models).
/// For code, this tends to be conservative (code tokens are shorter).
///
/// PATTERN_DECISION: Level 1 (Pure Function)
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4 // ceil division
}

/// Estimate tokens for a structured item with name, kind, and file path.
pub fn estimate_node_tokens(name: &str, kind: &str, file: &str) -> usize {
    estimate_tokens(name) + estimate_tokens(kind) + estimate_tokens(file) + 8
}

// ---------------------------------------------------------------------------
// Context compressor
// ---------------------------------------------------------------------------

/// Truncate a list of items to fit within a token budget.
///
/// Returns `(truncated_items, was_truncated)`.
/// Items are taken in order — priority should be established by the caller (sort first).
///
/// PATTERN_DECISION: Level 1 (Pure Function)
pub fn fit_to_budget<T, F>(items: Vec<T>, budget: usize, estimate: F) -> (Vec<T>, bool)
where
    F: Fn(&T) -> usize,
{
    let mut result = Vec::new();
    let mut used: usize = 0;
    let total = items.len();

    for item in items {
        let cost = estimate(&item);
        if used + cost > budget {
            let result_len = result.len();
            return (result, result_len < total);
        }
        used += cost;
        result.push(item);
    }

    let truncated = result.len() < total;
    (result, truncated)
}

/// Truncate a string to approximately `max_tokens` tokens.
///
/// Appends "… [truncated]" if truncation occurred.
///
/// PATTERN_DECISION: Level 1 (Pure Function)
pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens * 4;
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}… [truncated]", &text[..max_chars])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello"), 2); // 5 chars → ceil(5/4) = 2
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars → ceil(11/4) = 3
    }

    #[test]
    fn fit_to_budget_truncates() {
        let items: Vec<String> = (0..10).map(|i| format!("item_{i:04}")).collect();
        let (result, truncated) = fit_to_budget(items, 20, |s| estimate_tokens(s));
        assert!(truncated);
        assert!(result.len() < 10);
    }

    #[test]
    fn fit_to_budget_fits_all() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (result, truncated) = fit_to_budget(items, 100, |s| estimate_tokens(s));
        assert!(!truncated);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn truncate_to_tokens_short() {
        let s = truncate_to_tokens("short", 100);
        assert_eq!(s, "short");
    }

    #[test]
    fn truncate_to_tokens_long() {
        let long = "a".repeat(1000);
        let s = truncate_to_tokens(&long, 10);
        assert!(s.contains("[truncated]"));
        assert!(s.len() < 100);
    }
}

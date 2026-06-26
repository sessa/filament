//! A small, self-contained fuzzy matcher for the sidebar filter.
//!
//! We deliberately avoid a heavy fuzzy-match dependency: for a config dashboard
//! with tens-to-hundreds of items, a subsequence matcher with word-boundary
//! bonuses is more than enough, fully deterministic, and trivially testable.

use filament_core::Entry;

/// Score `haystack` against `needle` (case-insensitive). Returns `None` if the
/// needle isn't a subsequence of the haystack; higher scores are better matches.
pub fn fuzzy_score(haystack: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().flat_map(char::to_lowercase).collect();
    let need: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();

    let mut hi = 0;
    let mut score = 0i32;
    let mut last_match: Option<usize> = None;

    for &nc in &need {
        let mut found = None;
        while hi < hay.len() {
            if hay[hi] == nc {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let idx = found?;
        score += 1;
        if last_match == Some(idx.wrapping_sub(1)) {
            score += 5; // consecutive characters
        }
        if idx == 0 {
            score += 8; // matches the very start
        } else if matches!(hay[idx - 1], ' ' | '-' | '_' | '/' | '.') {
            score += 6; // matches a word boundary
        }
        last_match = Some(idx);
        hi += 1;
    }

    // Slightly prefer shorter haystacks.
    score -= hay.len() as i32 / 20;
    Some(score)
}

/// Score an entry against a (possibly multi-token) query. Every whitespace token
/// must match the name, description, or kind; scores sum. An empty query matches
/// everything with score 0.
pub fn entry_match(entry: &Entry, query: &str) -> Option<i32> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    let kind = entry.kind.label();
    let desc = entry.description().unwrap_or("");

    let mut total = 0;
    for token in query.split_whitespace() {
        let best = [
            fuzzy_score(&entry.name, token).map(|s| s + 20),
            fuzzy_score(desc, token),
            fuzzy_score(kind, token),
        ]
        .into_iter()
        .flatten()
        .max()?;
        total += best;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn subsequence_required() {
        assert!(fuzzy_score("code-reviewer", "crv").is_some());
        assert!(fuzzy_score("code-reviewer", "xyz").is_none());
        assert!(fuzzy_score("anything", "").is_some());
    }

    #[test]
    fn prefix_beats_midword() {
        let prefix = fuzzy_score("deploy", "dep").unwrap();
        let mid = fuzzy_score("a-deploy", "dep").unwrap();
        assert!(prefix > mid);
    }
}

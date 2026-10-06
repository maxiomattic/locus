//! Local full-text search with a light fzf-style ranker.

/// Byte ranges of every case-insensitive, non-overlapping occurrence of
/// `query` in `text`, in order. Empty queries match nothing.
pub fn find_offsets(text: &str, query: &str) -> Vec<(usize, usize)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let hay = text.to_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = hay[from..].find(&q) {
        let start = from + at;
        out.push((start, start + q.len()));
        from = start + q.len();
    }
    out
}

pub fn note_score(title: &str, body: &str, query: &str) -> Option<i32> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    let mut total = 0;
    for token in query.split_whitespace() {
        if token.is_empty() {
            continue;
        }
        let title_hit = score_one(title, token).map(|s| s + 80);
        let body_hit = score_one(body, token);
        total += title_hit.into_iter().chain(body_hit).max()?;
    }
    Some(total)
}

fn score_one(haystack: &str, needle: &str) -> Option<i32> {
    let hay = haystack.to_lowercase();
    let ned = needle.to_lowercase();
    if ned.is_empty() {
        return Some(0);
    }
    if let Some(at) = hay.find(&ned) {
        let mut score = 1200 - (at.min(400) as i32);
        if at == 0 || hay.as_bytes().get(at.saturating_sub(1)).is_some_and(|b| !b.is_ascii_alphanumeric())
        {
            score += 60;
        }
        return Some(score);
    }
    subsequence(&hay, &ned)
}

fn subsequence(hay: &str, needle: &str) -> Option<i32> {
    let mut hchars = hay.char_indices();
    let mut score = 200;
    let mut prev = None;
    let mut first = None;
    for n in needle.chars() {
        let found = hchars.find(|(_, c)| *c == n);
        let (idx, _) = found?;
        if first.is_none() {
            first = Some(idx);
        }
        if let Some(p) = prev {
            if idx == p + 1 {
                score += 18;
            } else {
                score -= (idx.saturating_sub(p)).min(40) as i32;
            }
        }
        prev = Some(idx);
    }
    Some(score - first.unwrap_or(0).min(200) as i32)
}

#[cfg(test)]
mod tests {
    use super::{find_offsets, note_score};

    #[test]
    fn body_match_counts() {
        assert!(note_score("Alpha", "hello world", "world").is_some());
        assert!(note_score("Alpha", "hello world", "zzz").is_none());
    }

    #[test]
    fn title_outranks_body() {
        let title = note_score("lunch", "other", "lunch").unwrap();
        let body = note_score("other", "lunch", "lunch").unwrap();
        assert!(title > body);
    }

    #[test]
    fn fuzzy_subsequence() {
        assert!(note_score("Hello", "", "hlo").is_some());
        assert!(note_score("Grocery list", "milk", "grcry").is_some());
    }

    #[test]
    fn all_tokens_required() {
        assert!(note_score("A", "red blue", "red green").is_none());
        assert!(note_score("A", "red blue", "red blue").is_some());
    }

    #[test]
    fn find_offsets_handles_case_and_overlap() {
        let text = "Ab ab AB\nZab";
        // "ab ab ab\nzab" lowercased; matches at 0..2, 3..5, 6..8, 10..12
        assert_eq!(find_offsets(text, "ab"), vec![(0, 2), (3, 5), (6, 8), (10, 12)]);
        assert_eq!(find_offsets(text, "Zab"), vec![(9, 12)]);
        assert!(find_offsets(text, "").is_empty());
        assert!(find_offsets(text, "nope").is_empty());
    }
}

//! Matching what someone typed against names and paths, the way a file picker does: the
//! letters in order, anywhere, ranked by how well they line up.

/// How well some text matched what was typed, from [`fuzzy`]: a score to rank matches by,
/// higher being better, and which characters matched, to highlight them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Fuzzy {
    /// Higher is better. Only worth comparing with other scores for the same query.
    pub score: i64,
    /// Which characters matched, counted in `char`s from 0, in order.
    pub hits: Vec<usize>,
}

/// Whether `text` holds the letters of `query` in order, ignoring case and any spaces in
/// the query, and how well. Matches score higher where the letters run together, start
/// words (after `/`, `_`, `-`, `.` or a space, or a capital after a small letter), and fall
/// in the last part of a path, the file's own name; shorter texts win ties. An empty
/// query matches everything, the shortest first.
///
/// ```
/// use tinsel::text::fuzzy;
///
/// let hit = fuzzy("src/app/main.rs", "main").unwrap();
/// assert_eq!(hit.hits, [8, 9, 10, 11]);
/// assert!(fuzzy("src/app/main.rs", "mian").is_none());
/// // the file's own name beats a folder's
/// assert!(hit.score > fuzzy("main/src/app.rs", "main").unwrap().score);
/// ```
pub fn fuzzy(text: &str, query: &str) -> Option<Fuzzy> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let chars: Vec<char> = text.chars().collect();
    if query.is_empty() {
        return Some(Fuzzy {
            score: -(chars.len() as i64),
            hits: Vec::new(),
        });
    }
    let name_start = chars.iter().rposition(|c| *c == '/').map_or(0, |i| i + 1);
    let hits = subsequence(&chars[name_start..], &query)
        .map(|h| h.into_iter().map(|i| i + name_start).collect::<Vec<_>>())
        .or_else(|| subsequence(&chars, &query))?;
    let mut score: i64 = 0;
    for (n, &i) in hits.iter().enumerate() {
        score += 10;
        if i >= name_start {
            score += 12;
        }
        if n > 0 && hits[n - 1] + 1 == i {
            score += 18;
        }
        let starts_word = i == 0
            || matches!(chars[i - 1], '/' | '_' | '-' | '.' | ' ')
            || chars[i].is_uppercase() && chars[i - 1].is_lowercase();
        if starts_word {
            score += 20;
        }
        if i == name_start {
            score += 25;
        }
    }
    let spread = hits.last().unwrap_or(&0) - hits.first().unwrap_or(&0);
    score -= spread as i64;
    score -= chars.len() as i64 / 3;
    Some(Fuzzy { score, hits })
}

/// Where the letters of `query` (already lowercase) fall in `chars`, bunched together as
/// far as they'll go, if they're all there in order.
fn subsequence(chars: &[char], query: &[char]) -> Option<Vec<usize>> {
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    // the leftmost match, then each letter pulled right so they cluster
    let mut hits = Vec::with_capacity(query.len());
    let mut from = 0;
    for q in query {
        let at = (from..lower.len()).find(|&i| lower[i] == *q)?;
        hits.push(at);
        from = at + 1;
    }
    let mut limit = lower.len();
    for k in (0..hits.len()).rev() {
        let latest = (hits[k]..limit)
            .rev()
            .find(|&i| lower[i] == query[k])
            .unwrap_or(hits[k]);
        hits[k] = latest;
        limit = latest;
    }
    // and a letter just before the next one's place moves up to it
    for k in (0..hits.len().saturating_sub(1)).rev() {
        if hits[k] + 1 != hits[k + 1] && hits[k + 1] > 0 && lower[hits[k + 1] - 1] == query[k] {
            let lowest = if k == 0 { 0 } else { hits[k - 1] + 1 };
            if hits[k + 1] > lowest {
                hits[k] = hits[k + 1] - 1;
            }
        }
    }
    Some(hits)
}

/// `text` with the characters at `hits` (counted in `char`s from 0, as [`fuzzy`] gives
/// them) drawn by `hit` and every other one by `other`, which is told where it is too: to
/// show what matched a search.
///
/// ```
/// use tinsel::{bold, paint, palette};
/// use tinsel::text::{fuzzy, highlight, strip_ansi};
///
/// let name = "main.rs";
/// let found = fuzzy(name, "mrs").unwrap();
/// let shown = highlight(name, &found.hits, |s| bold(&paint(palette::ROSE, s)), |_, s| s.to_string());
/// assert_eq!(strip_ansi(&shown), "main.rs");
/// ```
pub fn highlight(
    text: &str,
    hits: &[usize],
    hit: impl Fn(&str) -> String,
    other: impl Fn(usize, &str) -> String,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut buf = [0; 4];
    for (i, ch) in text.chars().enumerate() {
        let s = ch.encode_utf8(&mut buf);
        out.push_str(&if hits.contains(&i) { hit(s) } else { other(i, s) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(text: &str, query: &str) -> Option<Vec<usize>> {
        fuzzy(text, query).map(|f| f.hits)
    }

    #[test]
    fn letters_in_order_bunched_together() {
        assert_eq!(hits("editor.rs", "edrs"), Some(vec![0, 1, 7, 8]));
        assert_eq!(hits("Editor.rs", "ED"), Some(vec![0, 1]));
        assert_eq!(hits("main.rs", "x"), None);
        assert_eq!(
            hits("main.rs", "m a"),
            Some(vec![0, 1]),
            "spaces in the query are left out"
        );
        assert_eq!(
            hits("src/ui/mod.rs", "uimod"),
            Some(vec![4, 5, 7, 8, 9]),
            "across folders"
        );
        assert_eq!(hits("ÉTÉ", "été"), Some(vec![0, 1, 2]));
    }

    #[test]
    fn better_matches_score_higher() {
        let score = |text: &str, query: &str| fuzzy(text, query).unwrap().score;
        assert!(
            score("src/finder.rs", "fin") > score("src/define.rs", "fin"),
            "a word's start"
        );
        assert!(
            score("a/finder.rs", "fr") < score("a/finder.rs", "fi"),
            "letters together"
        );
        assert!(
            score("short", "") > score("much longer", ""),
            "shorter first with nothing typed"
        );
    }

    #[test]
    fn hits_are_drawn_their_own_way() {
        let shown = highlight("abc", &[1], |s| format!("[{s}]"), |i, s| format!("{i}{s}"));
        assert_eq!(shown, "0a[b]2c");
    }
}

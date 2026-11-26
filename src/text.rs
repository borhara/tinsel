//! Measuring and shaping text by what the terminal shows, not by bytes or chars:
//! escape codes count for nothing, CJK and emoji count for two columns, and a character
//! with the marks, variation selectors and joiners that build on it counts as one.

mod fuzzy;
mod tables;

pub use fuzzy::{Fuzzy, fuzzy, highlight};

pub use crate::term::content_width;

/// A piece of text as the terminal takes it: an escape sequence, which shows nothing, or one
/// cluster (a character with the marks, selectors and joiners that build on it) and the
/// columns it takes up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token<'a> {
    /// A colour, cursor move, hyperlink or other escape sequence.
    Escape(&'a str),
    /// One cluster of visible text, and its width in columns.
    Text(&'a str, usize),
}

/// The text as [`Token`]s.
pub(crate) fn tokens(s: &str) -> Tokens<'_> {
    Tokens { s, at: 0 }
}

/// What [`tokens`] gives.
pub(crate) struct Tokens<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let rest = &self.s[self.at..];
        if rest.is_empty() {
            return None;
        }
        if rest.starts_with('\x1b') {
            let len = escape_len(rest);
            self.at += len;
            return Some(Token::Escape(&rest[..len]));
        }
        let (len, width) = cluster(rest);
        self.at += len;
        Some(Token::Text(&rest[..len], width))
    }
}

/// How long the escape sequence at the start of `s` is.
fn escape_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    match bytes.get(1) {
        // CSI: parameters, then one final byte in @..~
        Some(b'[') => bytes[2..]
            .iter()
            .position(|b| (b'@'..=b'~').contains(b))
            .map_or(s.len(), |i| i + 3),
        // OSC (hyperlinks, titles): ends at BEL or ESC \
        Some(b']') => {
            let mut i = 2;
            while i < bytes.len() {
                match bytes[i] {
                    0x07 => return i + 1,
                    0x1b if bytes.get(i + 1) == Some(&b'\\') => return i + 2,
                    _ => i += 1,
                }
            }
            s.len()
        }
        // ESC and one character
        Some(_) => 1 + s[1..].chars().next().map_or(0, char::len_utf8),
        None => 1,
    }
}

const ZWJ: char = '\u{200D}';
const VS16: char = '\u{FE0F}';

/// The cluster at the start of `s` (which isn't an escape), as its length in bytes and its
/// width in columns.
fn cluster(s: &str) -> (usize, usize) {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return (0, 0) };
    // tinsel's frames draw a tab as one space, so it's measured as one
    if first == '\t' {
        return (1, 1);
    }
    if first.is_control() {
        return (first.len_utf8(), 0);
    }
    let mut len = first.len_utf8();
    let mut width = char_width(first);
    // a pair of regional indicators is a flag
    if is_regional(first) {
        if let Some(second) = chars.next().filter(|&c| is_regional(c)) {
            return (len + second.len_utf8(), 2);
        }
        return (len, width);
    }
    let emoji = is_pictographic(first);
    let mut joined = false;
    for c in chars {
        if joined {
            // the emoji after a joiner is drawn as part of the one before it
            joined = false;
            if emoji && is_pictographic(c) {
                len += c.len_utf8();
                continue;
            }
            break;
        }
        if c == ZWJ {
            joined = true;
        } else if c == VS16 {
            // asks for an emoji's picture, which takes two columns
            width = width.max(2);
        } else if emoji && is_skin_tone(c) {
            // a skin tone colours the emoji before it
        } else if !is_zero_width(c) {
            break;
        }
        len += c.len_utf8();
    }
    (len, width)
}

fn is_regional(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

fn is_skin_tone(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

/// Characters that can be drawn as emoji, and so joined into one with a ZWJ: close to
/// Unicode's Extended_Pictographic.
fn is_pictographic(c: char) -> bool {
    matches!(c as u32,
        0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139 | 0x2194..=0x21AA | 0x231A..=0x23FF
        | 0x24C2 | 0x25AA..=0x25FE | 0x2600..=0x27BF | 0x2934 | 0x2935 | 0x2B05..=0x2B55
        | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0x1F000..=0x1F1E5 | 0x1F200..=0x1FAFF
        | 0x1FC00..=0x1FFFD)
}

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn is_zero_width(c: char) -> bool {
    (c as u32) >= 0x300 && in_table(tables::ZERO, c)
}

/// The text with every terminal escape (colours, cursor moves, hyperlinks) removed.
pub fn strip_ansi(s: &str) -> String {
    tokens(s)
        .filter_map(|t| match t {
            Token::Text(t, _) => Some(t),
            Token::Escape(_) => None,
        })
        .collect()
}

/// Columns a single character takes up, by Unicode 17's tables: two for wide (CJK and
/// emoji), none for combining marks, joiners, variation selectors and control characters,
/// one for the rest, a tab included (it's drawn as a space). Ambiguous characters take two
/// where the terminal draws them wide (see [`ambiguous_wide`](crate::ambiguous_wide)), but
/// for tinsel's own symbols, which go to such a terminal as ASCII. Text is
/// better measured with [`width`], which knows that a character and what builds on it, such
/// as a flag's two letters or a family joined into one emoji, are drawn as one.
pub fn char_width(c: char) -> usize {
    let n = c as u32;
    if c == '\t' {
        1
    } else if n < 0x20 || (0x7F..0xA0).contains(&n) {
        0
    } else if n < 0x300 {
        if n >= 0xA1 && wide_ambiguous(c) { 2 } else { 1 }
    } else if in_table(tables::ZERO, c) {
        0
    } else if in_table(tables::WIDE, c) || wide_ambiguous(c) {
        2
    } else {
        1
    }
}

// an ambiguous character that the terminal draws two columns wide
fn wide_ambiguous(c: char) -> bool {
    in_table(tables::AMBIGUOUS, c) && crate::glyphs::ambiguous_wide() && crate::glyphs::ascii_for(c).is_none()
}

/// Columns the text takes up on screen, ignoring escape codes.
pub fn width(s: &str) -> usize {
    tokens(s)
        .map(|t| match t {
            Token::Text(_, w) => w,
            Token::Escape(_) => 0,
        })
        .sum()
}

/// Pad on the right to `cols` visible columns. Styled text is fine.
pub fn pad(s: &str, cols: usize) -> String {
    format!("{s}{}", " ".repeat(cols.saturating_sub(width(s))))
}

/// Pad on the left to `cols` visible columns, for right-aligned numbers.
pub fn pad_left(s: &str, cols: usize) -> String {
    format!("{}{s}", " ".repeat(cols.saturating_sub(width(s))))
}

/// Cut text to at most `cols` columns, ending in "…" when anything was cut. Styled text is
/// fine: every escape is kept, so colours opened before the cut are closed after it.
pub fn truncate(s: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }
    if width(s) <= cols {
        return s.to_string();
    }
    let mut out = String::new();
    let (mut used, mut cut) = (0, false);
    for token in tokens(s) {
        match token {
            Token::Escape(e) => out += e,
            Token::Text(_, _) if cut => {}
            Token::Text(t, w) if used + w < cols => {
                out += t;
                used += w;
            }
            Token::Text(_, _) => {
                out += "…";
                cut = true;
            }
        }
    }
    out
}

/// Cut text to at most `cols` columns from the front, starting with "…" when anything was
/// cut: for a path, whose end says the most. Styled text is fine, as for [`truncate`].
///
/// ```
/// use tinsel::text::truncate_start;
///
/// assert_eq!(truncate_start("src/app/main.rs", 10), "…p/main.rs");
/// assert_eq!(truncate_start("main.rs", 10), "main.rs");
/// ```
pub fn truncate_start(s: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }
    if width(s) <= cols {
        return s.to_string();
    }
    let all: Vec<Token<'_>> = tokens(s).collect();
    // the last stretch of text that fits beside the "…"
    let mut used = 0;
    let mut start = all.len();
    for (i, token) in all.iter().enumerate().rev() {
        match *token {
            Token::Escape(_) => start = i,
            Token::Text(_, w) if used + w < cols => {
                used += w;
                start = i;
            }
            Token::Text(..) => break,
        }
    }
    let mut out: String = all[..start]
        .iter()
        .filter_map(|t| match t {
            Token::Escape(e) => Some(*e),
            Token::Text(..) => None,
        })
        .collect();
    out.push('…');
    for token in &all[start..] {
        match token {
            Token::Escape(t) | Token::Text(t, _) => out.push_str(t),
        }
    }
    out
}

/// Columns a character takes up when it's drawn at `column` (counted from 0): a tab reaches
/// the next tab stop, every `tab_stop` columns, and anything else is as wide as
/// [`char_width`] says.
///
/// ```
/// use tinsel::text::char_width_at;
///
/// assert_eq!(char_width_at('\t', 0, 4), 4);
/// assert_eq!(char_width_at('\t', 6, 4), 2);
/// assert_eq!(char_width_at('漢', 6, 4), 2);
/// ```
pub fn char_width_at(c: char, column: usize, tab_stop: usize) -> usize {
    if c == '\t' {
        let stop = tab_stop.max(1);
        stop - column % stop
    } else {
        char_width(c)
    }
}

/// Columns the text takes up with its tabs reaching the next tab stop, every `tab_stop`
/// columns, as an editor shows them. Escape codes count for nothing, as for [`width`].
///
/// ```
/// assert_eq!(tinsel::text::width_with_tabs("a\tb", 4), 5);
/// assert_eq!(tinsel::text::width_with_tabs("\t\t", 8), 16);
/// ```
pub fn width_with_tabs(s: &str, tab_stop: usize) -> usize {
    tokens(s).fold(0, |at, token| match token {
        Token::Text("\t", _) => at + char_width_at('\t', at, tab_stop),
        Token::Text(_, w) => at + w,
        Token::Escape(_) => at,
    })
}

/// The text with each tab replaced by the spaces that reach the next tab stop (every eight
/// columns), as a terminal would show it.
pub(crate) fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut col = 0;
    for token in tokens(s) {
        match token {
            Token::Text("\t", _) => {
                let n = 8 - col % 8;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            Token::Text(t, w) => {
                out += t;
                col = if t == "\n" { 0 } else { col + w };
            }
            Token::Escape(e) => out += e,
        }
    }
    out
}

/// Word-wrap plain text to `cols` columns. Words longer than a line are broken, between
/// characters and never inside one.
pub fn wrap(s: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let (mut line, mut used) = (String::new(), 0);
        for word in para.split_whitespace() {
            let w = width(word);
            if w > cols {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                used = 0;
                for token in tokens(word) {
                    match token {
                        Token::Escape(e) => line += e,
                        Token::Text(t, w) => {
                            if used + w > cols && used > 0 {
                                lines.push(std::mem::take(&mut line));
                                used = 0;
                            }
                            line += t;
                            used += w;
                        }
                    }
                }
                continue;
            }
            if !line.is_empty() && used + 1 + w > cols {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            if !line.is_empty() {
                line.push(' ');
                used += 1;
            }
            line += word;
            used += w;
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_without_escapes() {
        assert_eq!(width("\x1b[38;2;1;2;3mhi\x1b[39m"), 2);
        assert_eq!(width("\x1b]8;;https://x.y\x1b\\link\x1b]8;;\x1b\\"), 4);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("core · server"), 13);
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("hello world", 8), "hello w…");
        assert_eq!(truncate("short", 8), "short");
    }

    #[test]
    fn wraps_words_and_breaks_long_ones() {
        assert_eq!(wrap("the quick brown fox", 9), ["the quick", "brown fox"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("a\nb", 10), ["a", "b"]);
    }

    #[test]
    fn pads_styled_text_by_visible_width() {
        assert_eq!(width(&pad("\x1b[1mab\x1b[22m", 5)), 5);
        assert_eq!(pad_left("7", 3), "  7");
    }

    #[test]
    fn strips_every_kind_of_escape() {
        assert_eq!(strip_ansi("\x1b[1;31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1bcreset"), "reset");
        assert_eq!(strip_ansi("\x1b]0;title\x07after"), "after");
    }

    #[test]
    fn emoji_and_their_sequences_are_two_columns() {
        for s in ["🚀", "✅", "🫠", "❤️", "👨‍👩‍👧", "👍🏽", "🇬🇧", "1️⃣", "日"] {
            assert_eq!(width(s), 2, "{s:?}");
        }
        assert_eq!(width("❤"), 1);
        assert_eq!(width("🇬🇧🇫🇷"), 4);
    }

    #[test]
    fn marks_and_controls_take_no_columns() {
        assert_eq!(width("กั"), 1, "Thai vowel sign");
        assert_eq!(width("e\u{301}"), 1);
        assert_eq!(width("a\u{200B}b"), 2);
        assert_eq!(width("한\u{1161}"), 2);
        assert_eq!(char_width('\u{7}'), 0);
        assert_eq!(width("a\u{7}b"), 2);
    }

    #[test]
    fn tabs_are_a_column_until_expanded() {
        assert_eq!(width("ab\tc"), 4);
        assert_eq!(expand_tabs("ab\tc"), "ab      c");
        assert_eq!(expand_tabs("日\t\x1b[1mx\tz"), "日      \x1b[1mx       z");
        assert_eq!(expand_tabs("a\nb\tc"), "a\nb       c");
    }

    #[test]
    fn tokens_split_escapes_from_clusters() {
        let found: Vec<Token> = tokens("\x1b[1me\u{301}\x1b]8;;u\x07👨‍👩‍👧").collect();
        assert_eq!(
            found,
            [
                Token::Escape("\x1b[1m"),
                Token::Text("e\u{301}", 1),
                Token::Escape("\x1b]8;;u\x07"),
                Token::Text("👨‍👩‍👧", 2),
            ]
        );
        assert_eq!(tokens("\x1b").collect::<Vec<_>>(), [Token::Escape("\x1b")]);
        assert_eq!(tokens("\x1b[31").collect::<Vec<_>>(), [Token::Escape("\x1b[31")]);
    }

    #[test]
    fn truncating_keeps_escapes_and_whole_characters() {
        assert_eq!(truncate("\x1b[31mhello\x1b[0m", 3), "\x1b[31mhe…\x1b[0m");
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(truncate("日本語", 4), "日…");
        assert_eq!(truncate("e\u{301}e\u{301}e\u{301}", 2), "e\u{301}…");
        assert_eq!(truncate("🇬🇧🇫🇷", 3), "🇬🇧…");
    }

    #[test]
    fn wrapping_counts_columns_and_keeps_characters_whole() {
        assert_eq!(wrap("日本語日本語", 4), ["日本", "語日", "本語"]);
        assert_eq!(wrap("e\u{301}e\u{301}e\u{301}", 2), ["e\u{301}e\u{301}", "e\u{301}"]);
        assert!(wrap("日本語 日本語 日", 7).iter().all(|l| width(l) <= 7));
    }

    #[test]
    fn a_long_word_starts_its_own_line() {
        assert_eq!(wrap("ab abcdefghij", 4), ["ab", "abcd", "efgh", "ij"]);
    }

    #[test]
    fn cutting_from_the_front_keeps_the_end_and_any_colour() {
        assert_eq!(truncate_start("文書/メモ.md", 8), "…メモ.md");
        assert_eq!(truncate_start("文書/メモ.md", 6), "…モ.md");
        assert_eq!(truncate_start("文書/メモ.md", 5), "….md");
        assert_eq!(truncate_start("abc", 1), "…");
        assert_eq!(truncate_start("abc", 0), "");
        let styled = "\x1b[31mred\x1b[0m tail";
        let cut = truncate_start(styled, 7);
        assert_eq!(strip_ansi(&cut), "…d tail");
        assert!(cut.starts_with("\x1b[31m…") && cut.contains("\x1b[0m"), "{cut:?}");
    }

    #[test]
    fn tabs_reach_the_next_stop() {
        assert_eq!(width_with_tabs("ab\tc", 4), 5);
        assert_eq!(width_with_tabs("\x1b[1m\t\x1b[0mx", 4), 5);
        assert_eq!(width_with_tabs("\t", 0), 1, "a stop of 0 is taken as 1");
        assert_eq!(char_width_at('a', 3, 4), 1);
    }
}

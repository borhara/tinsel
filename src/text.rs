//! Measuring and shaping text by what the terminal shows, not by bytes or chars:
//! escape codes count for nothing, CJK and emoji count for two columns.

/// The text with every terminal escape (colours, cursor moves, hyperlinks) removed.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in @..~
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC (hyperlinks, titles): ends at BEL or ESC \
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Columns a single character takes up.
pub fn char_width(c: char) -> usize {
    match c as u32 {
        0 => 0,
        // combining marks, zero-width joiners and variation selectors
        0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F => 0,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Columns the text takes up on screen, ignoring escape codes.
pub fn width(s: &str) -> usize {
    strip_ansi(s).chars().map(char_width).sum()
}

/// Pad on the right to `cols` visible columns. Styled text is fine.
pub fn pad(s: &str, cols: usize) -> String {
    format!("{s}{}", " ".repeat(cols.saturating_sub(width(s))))
}

/// Pad on the left to `cols` visible columns, for right-aligned numbers.
pub fn pad_left(s: &str, cols: usize) -> String {
    format!("{}{s}", " ".repeat(cols.saturating_sub(width(s))))
}

/// Cut plain text to at most `cols` columns, ending in "…" when anything was cut.
pub fn truncate(s: &str, cols: usize) -> String {
    if width(s) <= cols {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_width(c);
        if used + w + 1 > cols {
            break;
        }
        out.push(c);
        used += w;
    }
    out + "…"
}

/// Word-wrap plain text to `cols` columns. Words longer than a line are broken.
pub fn wrap(s: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let mut word = word.to_string();
            while width(&word) > cols {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let split = word.char_indices().nth(cols).map_or(word.len(), |(i, _)| i);
                lines.push(word[..split].to_string());
                word = word[split..].to_string();
            }
            let needed = if line.is_empty() {
                width(&word)
            } else {
                width(&line) + 1 + width(&word)
            };
            if needed > cols {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line += &word;
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
        assert_eq!(width("tube · iPhone"), 13);
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
}

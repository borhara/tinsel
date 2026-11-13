//! Markdown drawn for the terminal, in tinsel's colours: for changelogs, long help text and
//! what's-new notes.

use crate::block::Block;
use crate::layout::{Code, link};
use crate::message::rule;
use crate::style::palette::{BLUSH, LINE, MUTED, ROSE, SOFT, TEXT};
use crate::style::{Rgb, bold, italic, paint};
use crate::table::Table;
use crate::term::content_width;
use crate::text;

/// Markdown as styled lines, wrapped to the terminal: headings, paragraphs, lists (nested,
/// numbered, and task lists), quotes, rules, tables, and fenced code blocks coloured by
/// language. Inline, it understands `**bold**`, `*italic*`, `` `code` `` and `[links](…)`.
///
/// ```
/// tinsel::markdown("## What's new\n\n- **Faster** builds\n- A `--json` flag").print();
/// ```
pub fn markdown(src: &str) -> Block {
    let width = content_width().saturating_sub(2);
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Block::new();
    // blocks are separated by exactly one blank line, however many the source has
    let gap = |out: &mut Block| {
        if !out.is_empty() && out.lines().last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // fenced code
        if let Some(fence) = ["```", "~~~"].into_iter().find(|f| trimmed.starts_with(f)) {
            let lang = trimmed.trim_start_matches(fence).trim();
            let body: Vec<&str> = lines[i + 1..]
                .iter()
                .take_while(|l| !l.trim().starts_with(fence))
                .copied()
                .collect();
            i += body.len() + 2;
            gap(&mut out);
            out.extend(Code::new(body.join("\n")).lang(lang).numbers(false).lines());
            continue;
        }

        // headings
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let title = trimmed[hashes..].trim().trim_end_matches('#').trim();
            gap(&mut out);
            let styled = match hashes {
                1 => bold(&paint(ROSE, title)),
                2 => bold(&paint(BLUSH, title)),
                _ => bold(&paint(TEXT, title)),
            };
            out.push(format!("  {styled}"));
            if hashes == 1 {
                out.push(format!("  {}", paint(LINE, &"─".repeat(text::width(title)))));
            }
            i += 1;
            continue;
        }

        // rules
        if trimmed.len() >= 3
            && ["-", "*", "_"]
                .iter()
                .any(|c| trimmed.chars().all(|x| x.to_string() == *c || x == ' '))
        {
            gap(&mut out);
            out.push(rule(""));
            i += 1;
            continue;
        }

        // tables: a header row, a row of dashes, then rows
        if trimmed.starts_with('|') && lines.get(i + 1).is_some_and(|l| is_table_rule(l)) {
            let cells = |l: &str| -> Vec<String> {
                l.trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| inline(c.trim(), TEXT))
                    .collect()
            };
            let mut table = Table::new(cells(line));
            let mut j = i + 2;
            while j < lines.len() && lines[j].trim().starts_with('|') {
                table = table.row(cells(lines[j]));
                j += 1;
            }
            gap(&mut out);
            out.extend(table.lines());
            i = j;
            continue;
        }

        // quotes
        if trimmed.starts_with('>') {
            let mut body = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('>') {
                body.push(lines[i].trim().trim_start_matches('>').trim());
                i += 1;
            }
            gap(&mut out);
            let bar = format!("  {} ", paint(MUTED, "▎"));
            out.extend(wrap(&words(&body.join(" "), SOFT), width.saturating_sub(2), &bar, &bar));
            continue;
        }

        // lists
        if list_item(line).is_some() {
            gap(&mut out);
            let mut number = 0;
            while i < lines.len() {
                let Some((level, marker, rest)) = list_item(lines[i]) else {
                    // a continuation line belongs to the item above
                    if !lines[i].trim().is_empty() && lines[i].starts_with("  ") && !out.is_empty() {
                        i += 1;
                        continue;
                    }
                    break;
                };
                let (bullet, rest) = match marker {
                    Marker::Bullet => (paint(ROSE, if level == 0 { "•" } else { "◦" }), rest.to_string()),
                    Marker::Number => {
                        number += 1;
                        (paint(BLUSH, &format!("{number}.")), rest.to_string())
                    }
                };
                // task lists
                let (bullet, rest) = if let Some(r) = rest.strip_prefix("[ ] ") {
                    (paint(MUTED, "○"), r.to_string())
                } else if let Some(r) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
                    (paint(ROSE, "●"), r.to_string())
                } else {
                    (bullet, rest)
                };
                let indent = "  ".repeat(level + 1);
                let lead = format!("{indent}{bullet} ");
                let hang = " ".repeat(text::width(&lead));
                out.extend(wrap(
                    &words(&rest, TEXT),
                    width.saturating_sub(hang.len()),
                    &lead,
                    &hang,
                ));
                i += 1;
            }
            continue;
        }

        // a paragraph: lines up to the next blank or block
        let mut para = Vec::new();
        while i < lines.len() {
            let l = lines[i].trim();
            let starts_block = l.is_empty()
                || l.starts_with('#')
                || l.starts_with('>')
                || l.starts_with("```")
                || l.starts_with('|')
                || list_item(lines[i]).is_some();
            if starts_block && !para.is_empty() {
                break;
            }
            para.push(l);
            i += 1;
        }
        gap(&mut out);
        out.extend(wrap(&words(&para.join(" "), TEXT), width, "  ", "  "));
    }
    out
}

enum Marker {
    Bullet,
    Number,
}

/// `- item`, `* item`, `1. item`: its nesting level, kind and text.
fn list_item(line: &str) -> Option<(usize, Marker, &str)> {
    let spaces = line.len() - line.trim_start().len();
    let t = line.trim_start();
    let level = spaces / 2;
    for b in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(b) {
            return Some((level, Marker::Bullet, rest.trim()));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let rest = &t[digits..];
    if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ")) {
        return Some((level, Marker::Number, rest[2..].trim()));
    }
    None
}

fn is_table_rule(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| "|-: ".contains(c))
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
}

/// A word: its plain text (for measuring) and how it's drawn.
struct Word {
    plain: String,
    painted: String,
}

/// Inline markup as words ready to wrap.
fn words(src: &str, base: Rgb) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    // whether the next character continues the last word (no space between)
    let mut joined = false;
    for (piece, style, url) in spans(src) {
        let paint_word = |w: &str| {
            let colored = if style.code { paint(BLUSH, w) } else { paint(base, w) };
            let colored = if style.bold { bold(&colored) } else { colored };
            let colored = if style.italic { italic(&colored) } else { colored };
            match &url {
                Some(u) => link(w, u),
                None => colored,
            }
        };
        let starts_with_space = piece.starts_with(char::is_whitespace);
        for (n, w) in piece.split_whitespace().enumerate() {
            // carry on the last word, as in **bold**ly
            if let Some(last) = out.last_mut().filter(|_| n == 0 && joined && !starts_with_space) {
                last.plain += w;
                last.painted += &paint_word(w);
                continue;
            }
            out.push(Word {
                plain: w.to_string(),
                painted: paint_word(w),
            });
        }
        joined = !piece.ends_with(char::is_whitespace) && !piece.is_empty();
    }
    out
}

/// Split inline markup into runs of text with one style each, plus a link target.
fn spans(src: &str) -> Vec<(String, Style, Option<String>)> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut style = Style::default();
    let mut buf = String::new();
    let mut i = 0;
    let flush = |buf: &mut String, style: Style, out: &mut Vec<(String, Style, Option<String>)>| {
        if !buf.is_empty() {
            out.push((std::mem::take(buf), style, None));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\\' if next.is_some() => {
                buf.push(chars[i + 1]);
                i += 2;
            }
            '`' => {
                let end = chars[i + 1..].iter().position(|&c| c == '`').map(|p| i + 1 + p);
                if let Some(end) = end {
                    flush(&mut buf, style, &mut out);
                    let code: String = chars[i + 1..end].iter().collect();
                    out.push((code, Style { code: true, ..style }, None));
                    i = end + 1;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '*' | '_' if next == Some(c) => {
                flush(&mut buf, style, &mut out);
                style.bold = !style.bold;
                i += 2;
            }
            // underscores inside words (snake_case) aren't emphasis
            '_' if i > 0 && chars[i - 1].is_alphanumeric() && next.is_some_and(char::is_alphanumeric) => {
                buf.push(c);
                i += 1;
            }
            '*' | '_' => {
                flush(&mut buf, style, &mut out);
                style.italic = !style.italic;
                i += 1;
            }
            '[' => {
                let close = chars[i..].iter().position(|&c| c == ']').map(|p| i + p);
                let paren = close.filter(|&c| chars.get(c + 1) == Some(&'('));
                let end = paren.and_then(|c| chars[c..].iter().position(|&x| x == ')').map(|p| c + p));
                if let (Some(close), Some(end)) = (paren, end) {
                    flush(&mut buf, style, &mut out);
                    let label: String = chars[i + 1..close].iter().collect();
                    let url: String = chars[close + 2..end].iter().collect();
                    out.push((label, style, Some(url)));
                    i = end + 1;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    flush(&mut buf, style, &mut out);
    out
}

/// Inline markup as one styled string, for table cells.
fn inline(src: &str, base: Rgb) -> String {
    words(src, base)
        .iter()
        .map(|w| w.painted.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fill lines with words up to `width` columns, the first line starting with `first` and the
/// rest with `rest`.
fn wrap(words: &[Word], width: usize, first: &str, rest: &str) -> Block {
    let width = width.max(10);
    let mut out = Vec::new();
    let (mut line, mut used) = (first.to_string(), 0);
    for w in words {
        let wlen = text::width(&w.plain);
        if used > 0 && used + 1 + wlen > width {
            out.push(std::mem::replace(&mut line, rest.to_string()));
            used = 0;
        }
        if used > 0 {
            line.push(' ');
            used += 1;
        }
        line += &w.painted;
        used += wlen;
    }
    out.push(line);
    out.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(md: &str) -> String {
        crate::set_color_mode(crate::ColorMode::None);
        markdown(md).plain()
    }

    #[test]
    fn draws_headings_lists_and_paragraphs() {
        let out = plain("# Title\n\nSome **bold** and `code`.\n\n- one\n  - nested\n- [x] done\n\n1. first\n2. second");
        assert_eq!(
            out,
            [
                "  Title",
                "  ─────",
                "",
                "  Some bold and code.",
                "",
                "  • one",
                "    ◦ nested",
                "  ● done",
                "",
                "  1. first",
                "  2. second",
            ]
            .join("\n")
        );
    }

    #[test]
    fn keeps_snake_case_and_punctuation_attached() {
        assert_eq!(
            plain("call `run()`, then my_fn_name."),
            "  call run(), then my_fn_name."
        );
    }

    #[test]
    fn draws_code_quotes_and_tables() {
        let out = plain("> a quote\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |");
        assert_eq!(
            out,
            ["  ▎ a quote", "", "  │ let x = 1;", "", "  a   b", "  ┈┈┈┈┈", "  1   2"].join("\n")
        );
    }

    #[test]
    fn headings_rules_and_task_lists() {
        let out = plain("## Second\n\n### Third\n\n---\n\n- [ ] todo\n- [x] done\n  more about it\n");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.contains(&"  Second") && lines.contains(&"  Third"), "{out}");
        assert!(lines.iter().any(|l| l.trim().starts_with("───")), "{out}");
        assert!(out.contains("○ todo") && out.contains("● done"), "{out}");
        assert!(!out.contains("more about it"), "{out}");
    }

    #[test]
    fn inline_markup_is_styled_and_taken_out() {
        let out = plain(r"*it* and **bold**ly, \*not\*, `open, [label](https://x.y), [just brackets] and [half](open");
        assert_eq!(
            // however wide the terminal, so wherever it wraps
            out.split_whitespace().collect::<Vec<_>>().join(" "),
            "it and boldly, *not*, `open, label, [just brackets] and [half](open"
        );
    }

    #[test]
    fn links_link_in_colour() {
        let out = crate::style::with_color_mode(crate::ColorMode::TrueColor, || markdown("[docs](https://docs.rs)"));
        assert!(out.lines()[0].contains("\x1b]8;;https://docs.rs"), "{out:?}");
    }

    #[test]
    fn long_paragraphs_wrap() {
        let out = plain(&"word ".repeat(40));
        assert!(out.lines().count() > 1, "{out}");
    }
}

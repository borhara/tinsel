//! Markdown drawn for the terminal, in tinsel's colours: for changelogs, long help text and
//! what's-new notes. [`markdown`](fn@crate::markdown) wraps it to the terminal, and
//! [`wrapped`] to a width of your own.

use crate::block::Block;
use crate::layout::{Code, link};
use crate::style::palette::{BLUSH, LINE, MUTED, ROSE, SOFT, TEXT};
use crate::style::{Line, Rgb, bold, italic, paint};
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
    wrapped(src, content_width())
}

/// Like [`markdown`](fn@crate::markdown), wrapped to `width` columns rather than the
/// terminal's: for a pane beside something else.
///
/// ```
/// let narrow = tinsel::markdown::wrapped("A paragraph long enough to wrap onto a second line.", 24);
/// assert!(narrow.lines().len() > 1);
/// ```
pub fn wrapped(src: &str, width: usize) -> Block {
    let width = width.saturating_sub(2);
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
            out.push(format!("  {}", paint(LINE, &"─".repeat(width))));
            i += 1;
            continue;
        }

        // tables: a header row, a row of dashes, then rows
        if trimmed.starts_with('|') && lines.get(i + 1).is_some_and(|l| is_table_rule(l)) {
            let cells = |l: &str| -> Vec<String> { split_row(l).iter().map(|c| inline(c.trim(), TEXT)).collect() };
            let headers = cells(line);
            let n = headers.len();
            let mut table = Table::new(headers);
            let mut j = i + 2;
            while j < lines.len() && lines[j].trim().starts_with('|') {
                // cells past the header's are dropped, as GFM has it
                let mut row = cells(lines[j]);
                row.truncate(n);
                table = table.row(row);
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
            // each item's lead and text, so a continuation line can join the item above it
            let mut items: Vec<(String, String)> = Vec::new();
            // the number each level is up to, or None where a level's list is bullets
            let mut numbers: Vec<Option<u64>> = Vec::new();
            while i < lines.len() {
                let Some((level, marker, rest)) = list_item(lines[i]) else {
                    // a continuation line belongs to the item above
                    if !lines[i].trim().is_empty() && lines[i].starts_with("  ") {
                        if let Some((_, text)) = items.last_mut() {
                            text.push(' ');
                            text.push_str(lines[i].trim());
                        }
                        i += 1;
                        continue;
                    }
                    break;
                };
                numbers.truncate(level + 1);
                numbers.resize(level + 1, None);
                let bullet = match marker {
                    Marker::Bullet => {
                        numbers[level] = None;
                        paint(ROSE, if level == 0 { "•" } else { "◦" })
                    }
                    Marker::Number(start) => {
                        // a list counts on from the number it starts at
                        let number = numbers[level].map_or(start, |n| n + 1);
                        numbers[level] = Some(number);
                        paint(BLUSH, &format!("{number}."))
                    }
                };
                // task lists
                let (bullet, rest) = if let Some(r) = rest.strip_prefix("[ ] ") {
                    (paint(MUTED, "○"), r)
                } else if let Some(r) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
                    (paint(ROSE, "●"), r)
                } else {
                    (bullet, rest)
                };
                let indent = "  ".repeat(level + 1);
                items.push((format!("{indent}{bullet} "), rest.to_string()));
                i += 1;
            }
            for (lead, rest) in items {
                let hang = " ".repeat(text::width(&lead));
                out.extend(wrap(
                    &words(&rest, TEXT),
                    width.saturating_sub(hang.len()),
                    &lead,
                    &hang,
                ));
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
    /// A numbered item, and its number in the source.
    Number(u64),
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
        let number = t[..digits].parse().unwrap_or(1);
        return Some((level, Marker::Number(number), rest[2..].trim()));
    }
    None
}

/// A table row's cells, split at its pipes but not at `\|` or at a pipe inside `` `code` ``.
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let chars: Vec<char> = t.chars().collect();
    let mut cells = vec![String::new()];
    // whether the row ended on a pipe, which closes the last cell rather than opening one
    let mut closed = false;
    let mut i = 0;
    while i < chars.len() {
        let cell = cells.last_mut().expect("there's always a cell");
        closed = false;
        match chars[i] {
            // an escape is kept for the inline markup to take out
            '\\' if i + 1 < chars.len() => {
                cell.push('\\');
                cell.push(chars[i + 1]);
                i += 2;
            }
            '`' => match chars[i + 1..].iter().position(|&c| c == '`') {
                Some(p) => {
                    // code keeps its backslashes, but `\|` is still a pipe
                    let code: String = chars[i..i + p + 2].iter().collect();
                    cell.push_str(&code.replace("\\|", "|"));
                    i += p + 2;
                }
                None => {
                    cell.push('`');
                    i += 1;
                }
            },
            '|' => {
                cells.push(String::new());
                closed = true;
                i += 1;
            }
            c => {
                cell.push(c);
                i += 1;
            }
        }
    }
    if closed {
        cells.pop();
    }
    cells
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
    // where each closing bracket, paren and backtick next comes, worked out once so that
    // looking for one is quick however many openers go unclosed
    let next = |want: char| {
        let mut at = vec![None; chars.len() + 1];
        for k in (0..chars.len()).rev() {
            at[k] = if chars[k] == want { Some(k) } else { at[k + 1] };
        }
        at
    };
    let (next_bracket, next_paren, next_tick) = (next(']'), next(')'), next('`'));
    let closers = Closers::new(&chars);
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
                let end = next_tick[i + 1];
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
            '*' | '_' if next == Some(c) && closers.toggles(&chars, i, 2, style.bold) => {
                flush(&mut buf, style, &mut out);
                style.bold = !style.bold;
                i += 2;
            }
            // underscores inside words (snake_case) aren't emphasis
            '_' if i > 0 && chars[i - 1].is_alphanumeric() && next.is_some_and(char::is_alphanumeric) => {
                buf.push(c);
                i += 1;
            }
            '*' | '_' if next != Some(c) && closers.toggles(&chars, i, 1, style.italic) => {
                flush(&mut buf, style, &mut out);
                style.italic = !style.italic;
                i += 1;
            }
            '[' => {
                let close = next_bracket[i];
                let paren = close.filter(|&c| chars.get(c + 1) == Some(&'('));
                let end = paren.and_then(|c| next_paren[c]);
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

/// Where the last run of each kind of emphasis mark that could close (one that comes after
/// text) starts, found once so each opening mark needn't look along the rest of the line.
struct Closers {
    /// For `*`, `**`, `_` and `__`, in that order.
    last: [Option<usize>; 4],
}

impl Closers {
    fn new(chars: &[char]) -> Self {
        let mut last = [None; 4];
        for (slot, (mark, len)) in [('*', 1), ('*', 2), ('_', 1), ('_', 2)].into_iter().enumerate() {
            last[slot] = (1..chars.len().saturating_sub(len - 1))
                .rev()
                .find(|&k| chars[k..k + len].iter().all(|&c| c == mark) && !chars[k - 1].is_whitespace());
        }
        Closers { last }
    }

    /// Whether the run of `len` emphasis marks at `i` opens emphasis (when `on` is false) or
    /// closes it. It opens only before text, with a closing run to come, and closes only
    /// after text, so the `*` in `2 * 3` is just a star.
    fn toggles(&self, chars: &[char], i: usize, len: usize, on: bool) -> bool {
        let after_text = i > 0 && !chars[i - 1].is_whitespace();
        if on {
            return after_text;
        }
        let before_text = chars.get(i + len).is_some_and(|c| !c.is_whitespace());
        let slot = usize::from(chars[i] == '_') * 2 + len - 1;
        before_text && self.last[slot].is_some_and(|k| k > i + len)
    }
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
    let width = width.max(1);
    let mut out = Vec::new();
    let (mut line, mut used) = (first.to_string(), 0);
    for w in words {
        let wlen = text::width(&w.plain);
        if wlen > width {
            // a word wider than the room is broken, its colours carried across each break
            if used > 0 {
                out.push(std::mem::replace(&mut line, rest.to_string()));
            }
            let pieces = Line::new().raw(&w.painted).wrap(width);
            let last = pieces.len() - 1;
            for (n, piece) in pieces.into_iter().enumerate() {
                line += &piece.to_string();
                used = piece.width();
                if n < last {
                    out.push(std::mem::replace(&mut line, rest.to_string()));
                }
            }
            continue;
        }
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
        assert!(out.contains("● done more about it"), "{out}");
    }

    #[test]
    fn numbers_count_on_from_the_source_at_each_level() {
        let out = plain("1. one\n   1. inner\n   2. inner two\n2. two\n\n5. five\n6. six");
        assert_eq!(
            out,
            [
                "  1. one",
                "    1. inner",
                "    2. inner two",
                "  2. two",
                "",
                "  5. five",
                "  6. six"
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_lone_star_is_just_a_star() {
        assert_eq!(plain("2 * 3 = 6 and 4 * 5"), "  2 * 3 = 6 and 4 * 5");
        assert_eq!(plain("a ** b"), "  a ** b");
        assert_eq!(plain("*it* and 5*3"), "  it and 5*3");
        assert_eq!(plain("**bold** _it_"), "  bold it");
    }

    #[test]
    fn narrow_panes_are_kept_to() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = wrapped("some words and a longishword here\n\n- an item that wraps", 8);
        for line in out.lines() {
            assert!(text::width(line) <= 8, "{line:?} in {out:?}");
        }
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
    fn table_rows_take_the_header_count() {
        let out = plain("| a | b |\n|---|---|\n| 1 | 2 | 3 |\n| 4 |");
        assert_eq!(out, ["  a   b", "  ┈┈┈┈┈", "  1   2", "  4   ·"].join("\n"));
    }

    #[test]
    fn table_cells_keep_escaped_pipes_and_pipes_in_code() {
        assert_eq!(
            split_row(r"| a \| b | `c | d` | e |"),
            [r" a \| b ", " `c | d` ", " e "]
        );
        assert_eq!(split_row(r"| `x \| y` | z"), [" `x | y` ", " z"]);
        assert_eq!(split_row("| a || b |"), [" a ", "", " b "]);
        let out = plain("| op | means |\n|---|---|\n| `a | b` | either \\| or |");
        assert!(out.ends_with("a | b   either | or"), "{out}");
    }

    #[test]
    fn unclosed_marks_stay_quick() {
        // each of these used to look along the rest of the line from every mark
        for unit in ["*a ", "_a ", "**a ", "[a](", "[[", "a`"] {
            let src = unit.repeat(40_000);
            assert!(plain(&src).len() >= src.len() / 2, "{unit}");
        }
        // no star follows text, so none of them is emphasis
        assert_eq!(plain(&"*a ".repeat(40_000)).matches('*').count(), 40_000);
        assert_eq!(
            plain(&format!("{}b*", "*a ".repeat(40_000))).matches('*').count(),
            40_000 - 1
        );
    }

    #[test]
    fn fenced_code_expands_tabs() {
        assert_eq!(plain("```\n\tx\n```"), "  │         x");
    }

    #[test]
    fn markdown_links_cannot_run_commands() {
        let out = crate::style::with_color_mode(crate::ColorMode::TrueColor, || {
            markdown("[docs](https://x\x1b\\\x1b]0;PWNED\x07\x1b[2J)")
        });
        let line = &out.lines()[0];
        assert!(!line.contains('\x07') && !line.contains("\x1b[2J"), "{line:?}");
        assert_eq!(text::strip_ansi(line).trim(), "docs");
    }

    #[test]
    fn long_paragraphs_wrap() {
        let out = plain(&"word ".repeat(40));
        assert!(out.lines().count() > 1, "{out}");
    }
}

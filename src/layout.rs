//! Small layout pieces: badges, links, aligned pairs, columns, diffs and code excerpts.

use std::fmt;

use crate::block::Block;
use crate::highlight::{self, Lang};
use crate::style::palette::{BLUSH, LINE, MUTED, RED, ROSE, SAGE, SOFT, TEXT, TRACK};
use crate::style::{ColorMode, Rgb, color_mode, mix, paint, paint_on, underline};
use crate::term::content_width;
use crate::text;

/// A small pill: ` live ` on a soft tint of the colour, with the colour as its text.
///
/// ```
/// use tinsel::{badge, palette};
/// println!("{} {}", badge(palette::ROSE, "live"), badge(palette::AMBER, "beta"));
/// ```
pub fn badge(color: Rgb, label: &str) -> String {
    paint_on(color, mix(TRACK, color, 0.28), &format!(" {label} "))
}

/// A clickable link in terminals that support them (OSC 8); just the label elsewhere.
///
/// ```
/// println!("docs: {}", tinsel::link("tinsel on docs.rs", "https://docs.rs/tinsel"));
/// ```
pub fn link(label: &str, url: &str) -> String {
    let label = clean_label(label);
    if color_mode() == ColorMode::None {
        return label;
    }
    format!(
        "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
        clean_url(url),
        underline(&paint(BLUSH, &label))
    )
}

/// Control characters (C0, DEL and C1) can't end the link early or run other commands.
fn is_control(c: char) -> bool {
    c < ' ' || ('\x7f'..='\u{9f}').contains(&c)
}

/// The url with its control characters percent-encoded, so it can't break out of the link.
fn clean_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        if is_control(c) {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out += &format!("%{b:02X}");
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The label with its colours kept but any other escape or control character taken out.
fn clean_label(label: &str) -> String {
    text::tokens(label)
        .filter_map(|t| match t {
            text::Token::Escape(e) if e.starts_with("\x1b[") && e.ends_with('m') => Some(e),
            text::Token::Escape(_) => None,
            text::Token::Text(t, _) if t.chars().any(is_control) => None,
            text::Token::Text(t, _) => Some(t),
        })
        .collect()
}

/// Keys in a muted column as wide as the longest, values beside them.
///
/// ```
/// tinsel::pairs(&[("region", "eu-west-2"), ("memory", "512 MB")]).print();
/// ```
pub fn pairs(pairs: &[(&str, &str)]) -> Block {
    let w = pairs.iter().map(|(k, _)| text::width(k)).max().unwrap_or(0) + 3;
    pairs
        .iter()
        .map(|(k, v)| format!("  {}{}", paint(MUTED, &text::pad(k, w)), paint(TEXT, v)))
        .collect()
}

/// Items laid out in as many columns as fit, filling down then across, like `ls`.
///
/// ```
/// tinsel::columns(&["Cargo.toml", "README.md", "src", "target"]).print();
/// ```
pub fn columns(items: &[&str]) -> Block {
    if items.is_empty() {
        return Block::new();
    }
    let cell = items.iter().map(|s| text::width(s)).max().unwrap_or(0) + 3;
    let cols = (content_width().saturating_sub(2) / cell).max(1);
    let rows = items.len().div_ceil(cols);
    (0..rows)
        .map(|r| {
            let line: String = (0..cols)
                .filter_map(|c| items.get(c * rows + r))
                .map(|s| text::pad(&paint(TEXT, s), cell))
                .collect();
            format!("  {}", line.trim_end())
        })
        .collect()
}

/// A line diff between two texts: removed lines in red, added in sage, unchanged dim.
///
/// ```
/// let d = tinsel::diff("memory = 256\n", "memory = 512\n");
/// assert_eq!(d.plain(), "  -  memory = 256\n  +  memory = 512");
/// ```
pub fn diff(old: &str, new: &str) -> Block {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // lines are compared over and over, so each is numbered once and the numbers compared
    let mut ids = std::collections::HashMap::new();
    let mut numbered: Vec<usize> = a
        .iter()
        .chain(&b)
        .map(|line| {
            let next = ids.len();
            *ids.entry(*line).or_insert(next)
        })
        .collect();
    let nb = numbered.split_off(a.len());
    let na = numbered;
    let mut kept = Vec::new();
    common(&na, &nb, (0, 0), &mut kept);
    let same = |line: &str| format!("  {}  {}", paint(MUTED, " "), paint(MUTED, line));
    let removed = |line: &str| format!("  {}  {}", paint(RED, "-"), paint(RED, line));
    let added = |line: &str| format!("  {}  {}", paint(SAGE, "+"), paint(SAGE, line));
    let (mut i, mut j, mut out) = (0, 0, Block::new());
    // between kept lines, removals come first, so a changed line reads "- old" then "+ new"
    for (x, y) in kept.into_iter().chain([(a.len(), b.len())]) {
        a[i..x].iter().for_each(|l| out.push(removed(l)));
        b[j..y].iter().for_each(|l| out.push(added(l)));
        if x < a.len() {
            out.push(same(a[x]));
        }
        (i, j) = (x + 1, y + 1);
    }
    out
}

/// The pairs of lines `a` and `b` keep in common (as positions from `at`), in order: a
/// shortest edit script by Myers' algorithm, split at its middle snake so it needs memory
/// in proportion to the texts, not to their product.
fn common<T: PartialEq>(a: &[T], b: &[T], at: (usize, usize), kept: &mut Vec<(usize, usize)>) {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    kept.extend((0..head).map(|i| (at.0 + i, at.1 + i)));
    let (a, b, at) = (&a[head..], &b[head..], (at.0 + head, at.1 + head));
    let tail = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[..a.len() - tail], &b[..b.len() - tail]);
    if !a.is_empty() && !b.is_empty() {
        let (x, y, u, v) = middle_snake(a, b);
        common(&a[..x], &b[..y], at, kept);
        kept.extend((0..u - x).map(|i| (at.0 + x + i, at.1 + y + i)));
        common(&a[u..], &b[v..], (at.0 + u, at.1 + v), kept);
    }
    kept.extend((0..tail).map(|i| (at.0 + a.len() + i, at.1 + b.len() + i)));
}

/// Where a shortest edit script from `a` to `b` crosses its middle: a run of equal lines
/// from `(x, y)` to `(u, v)`, found by searching from both ends at once. `a` and `b` aren't
/// empty and differ at both ends, so either side of it is a smaller edit.
fn middle_snake<T: PartialEq>(a: &[T], b: &[T]) -> (usize, usize, usize, usize) {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let delta = n - m;
    let off = n + m + 1;
    // the furthest x reached on each diagonal k = x - y, from the start and (counting
    // back) from the end
    let mut fwd = vec![0isize; 2 * off as usize + 1];
    let mut back = vec![0isize; 2 * off as usize + 1];
    let at = |k: isize| (off + k) as usize;
    for d in 0..=(n + m + 1) / 2 {
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && fwd[at(k - 1)] < fwd[at(k + 1)]) {
                fwd[at(k + 1)]
            } else {
                fwd[at(k - 1)] + 1
            };
            let (x0, y0) = (x, x - k);
            while x < n && x - k < m && a[x as usize] == b[(x - k) as usize] {
                x += 1;
            }
            fwd[at(k)] = x;
            let r = delta - k;
            if delta % 2 != 0 && r.abs() < d && x + back[at(r)] >= n {
                return (x0 as usize, y0 as usize, x as usize, (x - k) as usize);
            }
        }
        for r in (-d..=d).step_by(2) {
            let mut x = if r == -d || (r != d && back[at(r - 1)] < back[at(r + 1)]) {
                back[at(r + 1)]
            } else {
                back[at(r - 1)] + 1
            };
            let (x0, y0) = (x, x - r);
            while x < n && x - r < m && a[(n - 1 - x) as usize] == b[(m - 1 - (x - r)) as usize] {
                x += 1;
            }
            back[at(r)] = x;
            let k = delta - r;
            if delta % 2 == 0 && k.abs() <= d && fwd[at(k)] + x >= n {
                return (
                    (n - x) as usize,
                    (m - (x - r)) as usize,
                    (n - x0) as usize,
                    (m - y0) as usize,
                );
            }
        }
    }
    unreachable!("an edit script from a to b is at most a.len() + b.len() long")
}

/// Source lines with numbers in a gutter, and optionally one line marked, with a caret under
/// the part that matters: the shape compilers use for errors.
///
/// ```
/// use tinsel::Code;
///
/// let src = "fn main() {\n    run();\n}";
/// let excerpt = Code::new(src).first_line(12).highlight(13);
/// excerpt.print();
/// assert!(excerpt.lines().plain().contains("13 ›     run();"));
/// ```
///
/// [`Code::around`] cuts the excerpt out of a whole file for you:
///
/// ```
/// use tinsel::Code;
///
/// let config = "[server]\nport = \"eighty\"\nhost = \"0.0.0.0\"\n";
/// let excerpt = Code::around(config, 2, 1).path("app.toml").caret(8, 8, "expected a number");
/// assert!(excerpt.lines().plain().contains("^^^^^^^^ expected a number"));
/// ```
#[derive(Clone, Debug)]
pub struct Code {
    src: String,
    first_line: usize,
    highlight: Option<usize>,
    path: Option<String>,
    caret: Option<(usize, usize, String)>,
    lang: Option<Lang>,
    numbers: bool,
}

impl Code {
    /// An excerpt of `src`, numbered from 1.
    pub fn new(src: impl Into<String>) -> Self {
        Code {
            src: src.into(),
            first_line: 1,
            highlight: None,
            path: None,
            caret: None,
            lang: None,
            numbers: true,
        }
    }

    /// The lines around line `line` of a whole file (`context` lines either side), with
    /// `line` marked.
    pub fn around(file: &str, line: usize, context: usize) -> Self {
        let line = line.max(1);
        let first = line.saturating_sub(context).max(1);
        // saturating, so a huge line or context takes the rest of the file rather than overflowing
        let count = line.saturating_add(context).saturating_add(1).saturating_sub(first);
        let excerpt: Vec<&str> = file.lines().skip(first - 1).take(count).collect();
        Code::new(excerpt.join("\n")).first_line(first).highlight(line)
    }

    /// Number the first line of the excerpt as `n`, as when showing lines from inside a file.
    #[must_use]
    pub fn first_line(mut self, n: usize) -> Self {
        self.first_line = n;
        self
    }

    /// Mark line `n` (in the same numbering as the gutter).
    #[must_use]
    pub fn highlight(mut self, n: usize) -> Self {
        self.highlight = Some(n);
        self
    }

    /// Name the file the excerpt comes from, shown above it with the marked line's number.
    /// If its extension is one tinsel knows ([`Lang`]), the code is coloured as that language.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.lang = self.lang.or_else(|| Lang::from_path(&path));
        self.path = Some(path);
        self
    }

    /// Colour the code as a language: keywords, strings, numbers, comments and config keys.
    /// Takes a [`Lang`] or a name like `"rust"` or `"toml"`; a name tinsel doesn't know
    /// leaves the code plain.
    ///
    /// ```
    /// tinsel::Code::new("let port = 8080; // default").lang("rust").print();
    /// ```
    #[must_use]
    pub fn lang(mut self, lang: impl IntoLang) -> Self {
        self.lang = lang.into_lang();
        self
    }

    /// Leave out the line numbers, keeping a plain bar down the left (default: numbered).
    #[must_use]
    pub fn numbers(mut self, show: bool) -> Self {
        self.numbers = show;
        self
    }

    /// A line of the source, coloured if there's a language, quieter unless it's marked.
    fn paint_line(&self, line: &str, marked: bool) -> String {
        match self.lang {
            Some(lang) => highlight::line(line, lang, !marked),
            None => paint(if marked { TEXT } else { SOFT }, line),
        }
    }

    /// Underline `len` characters of the marked line, starting at column `col` (from 1), with
    /// a short label beside the carets.
    #[must_use]
    pub fn caret(mut self, col: usize, len: usize, label: impl Into<String>) -> Self {
        self.caret = Some((col.max(1), len.max(1), label.into()));
        self
    }

    /// The excerpt as lines.
    pub fn lines(&self) -> Block {
        let last = self.first_line + self.src.lines().count().saturating_sub(1);
        let w = if self.numbers { last.to_string().len() } else { 0 };
        let mut out = Block::new();
        if let Some(path) = &self.path {
            let at = self.highlight.map(|n| format!(":{n}")).unwrap_or_default();
            out.push(format!(
                "  {}{} {}",
                " ".repeat(if self.numbers { w + 1 } else { 0 }),
                paint(LINE, "╭─"),
                paint(SOFT, &format!("{path}{at}"))
            ));
        }
        for (i, raw) in self.src.lines().enumerate() {
            // tabs are drawn as the spaces they reach, so carets below line up with them
            let line = &text::expand_tabs(raw);
            let n = self.first_line + i;
            let num = if self.numbers {
                format!("{n:>w$} ")
            } else {
                String::new()
            };
            if Some(n) != self.highlight {
                out.push(format!(
                    "  {}{} {}",
                    paint(MUTED, &num),
                    paint(LINE, "│"),
                    self.paint_line(line, false)
                ));
                continue;
            }
            out.push(format!(
                "  {}{} {}",
                paint(ROSE, &num),
                paint(ROSE, "›"),
                self.paint_line(line, true)
            ));
            if let Some((col, len, label)) = &self.caret {
                // measured in columns, so wide characters before the carets keep them in place
                // columns count from the source's characters, a tab as one
                let before: String = raw.chars().take(col - 1).collect();
                let end = (col - 1).saturating_add(*len);
                let through: String = raw.chars().take(end).collect();
                let (before, through) = (text::expand_tabs(&before), text::expand_tabs(&through));
                // past the end of the line, a character is taken as one column
                let short = end.saturating_sub(raw.chars().count().max(col - 1));
                let carets = text::width(&through) - text::width(&before) + short;
                let label = if label.is_empty() {
                    String::new()
                } else {
                    format!(" {label}")
                };
                out.push(format!(
                    "  {}{} {}{}",
                    " ".repeat(if self.numbers { w + 1 } else { 0 }),
                    paint(LINE, "│"),
                    " ".repeat(text::width(&before)),
                    paint(ROSE, &format!("{}{label}", "^".repeat(carets.max(1))))
                ));
            }
        }
        out
    }

    /// Print the excerpt.
    pub fn print(&self) {
        self.lines().print();
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lines().fmt(f)
    }
}

/// Anything [`Code::lang`] accepts: a [`Lang`], or a language's name. Sealed: tinsel decides
/// what counts, so it can add more without breaking anything.
pub trait IntoLang: sealed::Sealed {
    /// The language, if it's one tinsel knows.
    fn into_lang(self) -> Option<Lang>;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Lang {}
    impl Sealed for &str {}
    impl Sealed for String {}
    impl Sealed for &String {}
}

impl IntoLang for Lang {
    fn into_lang(self) -> Option<Lang> {
        Some(self)
    }
}

impl IntoLang for &str {
    fn into_lang(self) -> Option<Lang> {
        Lang::from_name(self)
    }
}

impl IntoLang for String {
    fn into_lang(self) -> Option<Lang> {
        Lang::from_name(&self)
    }
}

impl IntoLang for &String {
    fn into_lang(self) -> Option<Lang> {
        Lang::from_name(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_come_by_value_or_by_name() {
        let name = String::from("Python");
        assert_eq!(
            [
                Lang::Rust.into_lang(),
                "toml".into_lang(),
                (&name).into_lang(),
                name.into_lang()
            ],
            [
                Some(Lang::Rust),
                Some(Lang::Toml),
                Some(Lang::Python),
                Some(Lang::Python)
            ]
        );
        assert_eq!("cobol".into_lang(), None);
    }

    #[test]
    fn links_are_only_links_in_colour() {
        let url = "https://example.com";
        assert_eq!(
            crate::style::with_color_mode(ColorMode::None, || link("site", url)),
            "site"
        );
        let linked = crate::style::with_color_mode(ColorMode::TrueColor, || link("site", url));
        assert!(linked.starts_with("\x1b]8;;https://example.com\x1b\\"), "{linked:?}");
    }

    #[test]
    fn links_cannot_break_out_of_themselves() {
        let url = "https://x\x1b\\\x1b]0;PWNED\x07\x1b[2J\u{9b}";
        let label = "do\x1b]0;t\x07c\rs";
        let linked = crate::style::with_color_mode(ColorMode::TrueColor, || link(label, url));
        assert!(
            linked.starts_with("\x1b]8;;https://x%1B\\%1B]0;PWNED%07%1B[2J%C2%9B\x1b\\"),
            "{linked:?}"
        );
        // only the link's own two sequences are OSC, and the rest are colours
        assert_eq!(linked.matches("\x1b]").count(), 2, "{linked:?}");
        assert!(!linked.contains('\x07') && !linked.contains('\r'), "{linked:?}");
        assert_eq!(text::strip_ansi(&linked), "docs");
        let plain = crate::style::with_color_mode(ColorMode::None, || link(label, url));
        assert_eq!(plain, "docs");
        // a label's own colours are kept
        let styled = crate::style::with_color_mode(ColorMode::TrueColor, || {
            let label = crate::style::bold("site");
            (link(&label, "https://x.y"), label)
        });
        assert!(styled.0.contains(&styled.1), "{styled:?}");
    }

    #[test]
    fn excerpts_around_a_line_never_overflow() {
        crate::set_color_mode(ColorMode::None);
        let out = Code::around("a\nb\nc", 2, usize::MAX).lines().plain();
        assert_eq!(out, "  1 │ a\n  2 › b\n  3 │ c");
        let out = Code::around("a\nb", usize::MAX, usize::MAX).lines().plain();
        assert_eq!(out, "  1 │ a\n  2 │ b");
    }

    #[test]
    fn carets_line_up_after_tabs() {
        crate::set_color_mode(ColorMode::None);
        let out = Code::new("\tx = 1").highlight(1).caret(2, 1, "").lines().plain();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "  1 ›         x = 1");
        assert_eq!(lines[1], "    │         ^");
        // a tab under the carets is as wide as the spaces it reaches
        let out = Code::new("a\tb").highlight(1).caret(2, 1, "").lines().plain();
        assert!(out.ends_with("│  ^^^^^^^"), "{out}");
    }

    #[test]
    fn no_items_make_no_columns() {
        assert!(columns(&[]).is_empty());
    }

    #[test]
    fn a_caret_without_a_label_just_points() {
        crate::set_color_mode(ColorMode::None);
        let code = Code::around("let x = 1;", 1, 0).lang(Lang::Rust).caret(5, 1, "");
        let out = code.lines().plain();
        assert!(out.lines().last().unwrap().trim_end().ends_with('^'), "{out}");
        assert_eq!(code.to_string(), code.lines().to_string());
        assert!(crate::output::capture(|| code.print()).contains("let x = 1;"));
    }

    #[test]
    fn carets_underline_wide_characters_in_full() {
        crate::set_color_mode(ColorMode::None);
        let out = Code::new("名前 = 1").highlight(1).caret(1, 2, "").lines().plain();
        assert!(out.lines().last().unwrap().ends_with("│ ^^^^"), "{out}");
        let out = Code::new("ab").highlight(1).caret(2, 3, "").lines().plain();
        assert!(out.lines().last().unwrap().ends_with("│  ^^^"), "{out}");
    }

    #[test]
    fn diffs_are_shortest_and_put_removals_first() {
        crate::set_color_mode(ColorMode::None);
        let d = diff("a\nb\nc\nd\ne", "a\nx\nc\ne\nf").plain();
        assert_eq!(d, "     a\n  -  b\n  +  x\n     c\n  -  d\n     e\n  +  f");
        assert_eq!(diff("", "").plain(), "");
        assert_eq!(diff("a\nb", "").plain(), "  -  a\n  -  b");
        assert_eq!(diff("", "a").plain(), "  +  a");
        let d = diff("x\na\nb\ny", "a\nz\nb").plain();
        assert_eq!(d, "  -  x\n     a\n  +  z\n     b\n  -  y");
    }

    #[test]
    fn diffs_match_the_longest_common_run_of_lines() {
        // a small generator, so the check covers many shapes of edit
        let mut seed = 7u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let words = ["a", "b", "c", "d"];
        for _ in 0..300 {
            let mut text = || {
                let n = next() % 9;
                (0..n).map(|_| words[(next() % 4) as usize]).collect::<Vec<_>>()
            };
            let (a, b) = (text(), text());
            let mut kept = Vec::new();
            common(&a, &b, (0, 0), &mut kept);
            assert!(kept.iter().all(|&(x, y)| a[x] == b[y]), "{a:?} {b:?}");
            assert!(
                kept.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1),
                "{a:?} {b:?}"
            );
            // the longest common subsequence, the slow way
            let mut lcs = vec![vec![0; b.len() + 1]; a.len() + 1];
            for i in (0..a.len()).rev() {
                for j in (0..b.len()).rev() {
                    lcs[i][j] = if a[i] == b[j] {
                        lcs[i + 1][j + 1] + 1
                    } else {
                        lcs[i + 1][j].max(lcs[i][j + 1])
                    };
                }
            }
            assert_eq!(kept.len(), lcs[0][0], "{a:?} {b:?}");
        }
    }

    #[test]
    fn long_diffs_stay_small() {
        let old: String = (0..2_000).map(|i| format!("{i}\n")).collect();
        let new: String = (0..2_000).map(|i| format!("{}\n", i * 7 % 2_003)).collect();
        assert!(diff(&old, &new).lines().len() >= 2_000);
    }

    #[test]
    fn items_fill_columns_down_then_across() {
        crate::set_color_mode(ColorMode::None);
        assert_eq!(columns(&["a", "b"]).plain(), "  a   b");
    }

    #[test]
    fn excerpts_name_their_file_and_mark_a_line() {
        crate::set_color_mode(ColorMode::None);
        let code = Code::around("one\ntwo\nthree", 2, 1).path("notes.txt").lines().plain();
        assert!(code.starts_with("  ") && code.contains("╭─ notes.txt:2"), "{code}");
        assert!(code.contains("› two"), "{code}");
    }
}

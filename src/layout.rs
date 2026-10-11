//! Small layout pieces: badges, links, aligned pairs, columns, diffs and code excerpts.

use std::fmt;

use crate::block::Block;
use crate::highlight::{self, Lang};
use crate::style::palette::{BLUSH, LINE, MUTED, RED, ROSE, SAGE, SOFT, TEXT, TRACK};
use crate::style::{ColorMode, Rgb, color_mode, mix, paint, paint_bg, underline};
use crate::term::content_width;
use crate::text;

/// A small pill: ` live ` on a soft tint of the colour, with the colour as its text.
///
/// ```
/// use tinsel::{badge, palette};
/// println!("{} {}", badge("live", palette::ROSE), badge("beta", palette::AMBER));
/// ```
pub fn badge(label: &str, color: Rgb) -> String {
    paint_bg(color, mix(TRACK, color, 0.28), &format!(" {label} "))
}

/// A clickable link in terminals that support them (OSC 8); just the label elsewhere.
///
/// ```
/// println!("docs: {}", tinsel::link("tinsel on docs.rs", "https://docs.rs/tinsel"));
/// ```
pub fn link(label: &str, url: &str) -> String {
    if color_mode() == ColorMode::None {
        return label.to_string();
    }
    format!("\x1b]8;;{url}\x1b\\{}\x1b]8;;\x1b\\", underline(&paint(BLUSH, label)))
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
    // longest common subsequence table, filled from the end
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Block::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push(format!("  {}  {}", paint(MUTED, " "), paint(MUTED, a[i])));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] > lcs[i + 1][j]) {
            // ties go to removal, so a changed line reads "- old" then "+ new"
            out.push(format!("  {}  {}", paint(SAGE, "+"), paint(SAGE, b[j])));
            j += 1;
        } else {
            out.push(format!("  {}  {}", paint(RED, "-"), paint(RED, a[i])));
            i += 1;
        }
    }
    out
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
        let excerpt: Vec<&str> = file.lines().skip(first - 1).take(line + context + 1 - first).collect();
        Code::new(excerpt.join("\n")).first_line(first).highlight(line)
    }

    /// Number the first line of the excerpt as `n`, as when showing lines from inside a file.
    pub fn first_line(mut self, n: usize) -> Self {
        self.first_line = n;
        self
    }

    /// Mark line `n` (in the same numbering as the gutter).
    pub fn highlight(mut self, n: usize) -> Self {
        self.highlight = Some(n);
        self
    }

    /// Name the file the excerpt comes from, shown above it with the marked line's number.
    /// If its extension is one tinsel knows ([`Lang`]), the code is coloured as that language.
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
    pub fn lang(mut self, lang: impl IntoLang) -> Self {
        self.lang = lang.into_lang();
        self
    }

    /// Leave out the line numbers, keeping a plain bar down the left (default: numbered).
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
        for (i, line) in self.src.lines().enumerate() {
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
                let before: String = line.chars().take(col - 1).collect();
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
                    paint(ROSE, &format!("{}{label}", "^".repeat(*len)))
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

/// Anything [`Code::lang`] accepts: a [`Lang`], or a language's name.
pub trait IntoLang {
    /// The language, if it's one tinsel knows.
    fn into_lang(self) -> Option<Lang>;
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

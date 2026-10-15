use std::cmp::Ordering;
use std::fmt;

use crate::block::Block;
use crate::style::palette::{BLUSH, LINE, MUTED, ROSE, TEXT, TRACK};
use crate::style::{Rgb, bold, mix, on_bg, paint};
use crate::text;

/// Which side of a column cells hug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Text hugs the left edge: the default for words.
    Left,
    /// Text hugs the right edge: the default for numbers.
    Right,
}

/// How a [`Table`] is framed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Border {
    /// No frame: a dotted rule under the header and space between columns. The default.
    #[default]
    None,
    /// A soft rounded frame around the whole table, with space (not lines) between columns.
    Rounded,
    /// A grid: square corners and lines between every column, for when cells need edges.
    Square,
}

/// What to do with a cell wider than its column's limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overflow {
    Truncate,
    Wrap,
}

#[derive(Clone, Debug, Default)]
struct Row {
    cells: Vec<String>,
    highlight: bool,
}

/// Rows under a header, columns sized to their widest cell. Columns of numbers right-align
/// on their own; cells can be styled (badges, coloured text) and still line up.
///
/// ```no_run
/// use tinsel::{Border, Table};
///
/// Table::new(["app", "status", "size"])
///     .row(["tube", "live", "34.2 MB"])
///     .row(["music", "building", "12.8 MB"])
///     .sort_by(2)               // numerically, units and all
///     .highlight(0)             // the first row, after sorting
///     .footer(["2 apps", "", "47.0 MB"])
///     .border(Border::Rounded)
///     .print();
/// ```
#[derive(Clone, Debug, Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Row>,
    footer: Option<Vec<String>>,
    align: Vec<Option<Align>>,
    limits: Vec<Option<(usize, Overflow)>>,
    border: Border,
    title: Option<String>,
    striped: bool,
}

impl Table {
    /// A table with these column headers.
    pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Self {
        Table {
            headers: headers.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Add a row of cells. Cells can be plain or styled (a [`badge`](crate::badge), say).
    pub fn row<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self {
        self.rows.push(Row {
            cells: cells.into_iter().map(Into::into).collect(),
            highlight: false,
        });
        self
    }

    /// A closing row under a rule, for totals.
    pub fn footer<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self {
        self.footer = Some(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Override the automatic alignment of one column.
    pub fn align(mut self, column: usize, align: Align) -> Self {
        set(&mut self.align, column, Some(align));
        self
    }

    /// Cut cells in a column to at most `cols` wide, ending in `…`.
    pub fn max_width(mut self, column: usize, cols: usize) -> Self {
        set(&mut self.limits, column, Some((cols.max(1), Overflow::Truncate)));
        self
    }

    /// Wrap cells in a column onto more lines once they're wider than `cols`.
    pub fn wrap(mut self, column: usize, cols: usize) -> Self {
        set(&mut self.limits, column, Some((cols.max(1), Overflow::Wrap)));
        self
    }

    /// Draw a frame around the table (default: [`Border::None`]).
    pub fn border(mut self, border: Border) -> Self {
        self.border = border;
        self
    }

    /// A title, set into the top border (or above the table, without one).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Tint every other row faintly, to help the eye along wide tables.
    pub fn striped(mut self) -> Self {
        self.striped = true;
        self
    }

    /// Pick out row `index` (counting rows added so far, from 0) in the accent colour, on a
    /// soft tint, with a little `✦` beside it. Highlighted rows stay highlighted when sorted.
    pub fn highlight(mut self, index: usize) -> Self {
        if let Some(row) = self.rows.get_mut(index) {
            row.highlight = true;
        }
        self
    }

    /// Sort the rows added so far by a column, smallest first. Columns of numbers sort by
    /// value, understanding units: `900 KB` comes before `1.2 MB`, `45s` before `2m`.
    pub fn sort_by(mut self, column: usize) -> Self {
        self.sort(column, false);
        self
    }

    /// Like [`sort_by`](Table::sort_by), largest first.
    pub fn sort_by_desc(mut self, column: usize) -> Self {
        self.sort(column, true);
        self
    }

    fn sort(&mut self, column: usize, desc: bool) {
        let numeric = self.column_align(column) == Align::Right;
        let cell = |r: &Row| text::strip_ansi(r.cells.get(column).map_or("", String::as_str));
        self.rows.sort_by(|a, b| {
            let (a, b) = (cell(a), cell(b));
            let order = if numeric {
                // cells that aren't numbers go last whichever way round
                match (parse_number(&a), parse_number(&b)) {
                    (Some(x), Some(y)) => x.total_cmp(&y),
                    (Some(_), None) => return Ordering::Less,
                    (None, Some(_)) => return Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            } else {
                a.to_lowercase().cmp(&b.to_lowercase())
            };
            if desc { order.reverse() } else { order }
        });
    }

    fn column_align(&self, col: usize) -> Align {
        if let Some(Some(a)) = self.align.get(col) {
            return *a;
        }
        let numeric = self
            .rows
            .iter()
            .filter_map(|r| r.cells.get(col))
            .map(|c| text::strip_ansi(c))
            .all(|c| {
                let c = c.trim();
                c.is_empty() || c.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '$')
            });
        if numeric && !self.rows.is_empty() {
            Align::Right
        } else {
            Align::Left
        }
    }

    /// A cell's lines once its column's limit is applied.
    fn fit(&self, col: usize, raw: &str) -> Vec<String> {
        match self.limits.get(col).copied().flatten() {
            Some((cols, _)) if text::width(raw) <= cols => vec![raw.to_string()],
            Some((cols, Overflow::Truncate)) => vec![text::truncate(&text::strip_ansi(raw), cols)],
            Some((cols, Overflow::Wrap)) => text::wrap(&text::strip_ansi(raw), cols),
            None => vec![raw.to_string()],
        }
    }

    /// The table as lines.
    pub fn lines(&self) -> Block {
        let layout = self.layout();
        match self.border {
            Border::Square => self.grid(&layout),
            Border::None | Border::Rounded => self.soft(&layout),
        }
        .into()
    }

    /// Every cell fitted to its column, and how wide and which way round each column is.
    fn layout(&self) -> Layout {
        let cols = self
            .rows
            .iter()
            .map(|r| r.cells.len())
            .chain([self.headers.len()])
            .chain(self.footer.as_ref().map(Vec::len))
            .max()
            .unwrap_or(0);
        let fitted = |cells: &[String]| -> Vec<Vec<String>> {
            (0..cols)
                .map(|c| self.fit(c, cells.get(c).map_or("", String::as_str)))
                .collect()
        };
        let header = fitted(&self.headers);
        let rows: Vec<(Vec<Vec<String>>, bool)> = self.rows.iter().map(|r| (fitted(&r.cells), r.highlight)).collect();
        let footer = self.footer.as_deref().map(fitted);
        let widths: Vec<usize> = (0..cols)
            .map(|c| {
                let all = rows
                    .iter()
                    .map(|(r, _)| &r[c])
                    .chain([&header[c]])
                    .chain(footer.iter().map(|f| &f[c]));
                all.flat_map(|lines| lines.iter().map(|l| text::width(l)))
                    .max()
                    .unwrap_or(0)
                    .max(1)
            })
            .collect();
        let aligns = (0..cols).map(|c| self.column_align(c)).collect();
        Layout {
            cols,
            header,
            rows,
            footer,
            widths,
            aligns,
        }
    }

    /// The band behind body row `n`: a pink tint when highlighted, a faint one on alternate
    /// rows when striped.
    fn band(&self, n: usize, marked: bool) -> Option<Rgb> {
        if marked {
            Some(mix(STRIPE, ROSE, 0.2))
        } else if self.striped && n % 2 == 1 {
            Some(STRIPE)
        } else {
            None
        }
    }

    /// The default look, framed or not: space between columns, a dotted line under the
    /// header, soft pink edges, and a little ✦ beside highlighted rows.
    fn soft(&self, l: &Layout) -> Vec<String> {
        let boxed = self.border == Border::Rounded;
        let inner = l.widths.iter().sum::<usize>() + 3 * l.cols.saturating_sub(1);
        let frame = |s: &str| paint(mix(LINE, ROSE, 0.35), s);
        let dots = |s: &str| paint(mix(BLUSH, TRACK, 0.55), s);
        // what goes between the frame's edges: a 2-column lead (where ✦ goes), the row, 2 spaces
        let wrap = |lead: &str, inside: String, band: Option<Rgb>| {
            if boxed {
                let inside = format!("{lead}{}  ", text::pad(&inside, inner));
                let inside = band.map_or_else(|| inside.clone(), |b| on_bg(b, &inside));
                format!("  {} {inside}{}", frame("│"), frame("│"))
            } else if let Some(b) = band {
                format!("{lead}{}", on_bg(b, &text::pad(&inside, inner)))
            } else {
                format!("{lead}{}", inside.trim_end())
            }
        };
        let blank = || wrap("  ", String::new(), None);
        let rule = || wrap("  ", dots(&"┈".repeat(inner)), None);
        let mut out = Vec::new();

        let title = self.title.as_deref().map(|t| bold(&paint(ROSE, &format!("✦ {t}"))));
        if boxed {
            let across = inner + 5;
            out.push(match &title {
                Some(t) => {
                    let t = text::truncate(&text::strip_ansi(t), across.saturating_sub(4));
                    let rest = across.saturating_sub(text::width(&t) + 3);
                    format!(
                        "  {}{}{}",
                        frame("╭─ "),
                        bold(&paint(ROSE, &t)),
                        frame(&format!(" {}╮", "─".repeat(rest)))
                    )
                }
                None => format!("  {}", frame(&format!("╭{}╮", "─".repeat(across)))),
            });
        } else if let Some(t) = &title {
            out.push(format!("  {t}"));
        }

        let heading = Look::heading();
        for line in self.cells(l, &l.header, &heading, "   ") {
            out.push(wrap("  ", line, None));
        }
        out.push(rule());
        // once any row wraps, a blank line between rows shows where each one ends
        let divided = l.rows.iter().any(|(r, _)| height(r) > 1);
        for (n, (cells, marked)) in l.rows.iter().enumerate() {
            if divided && n > 0 {
                out.push(blank());
            }
            let look = Look::body(*marked);
            let band = self.band(n, *marked);
            for (i, line) in self.cells(l, cells, &look, "   ").into_iter().enumerate() {
                let lead = if *marked && i == 0 {
                    format!("{} ", paint(ROSE, "✦"))
                } else {
                    "  ".into()
                };
                out.push(wrap(&lead, line, band));
            }
        }
        if let Some(footer) = &l.footer {
            out.push(rule());
            for line in self.cells(l, footer, &Look::footer(), "   ") {
                out.push(wrap("  ", line, None));
            }
        }
        if boxed {
            out.push(format!("  {}", frame(&format!("╰{}╯", "─".repeat(inner + 5)))));
        }
        out
    }

    /// The grid look: every column edged, square corners.
    fn grid(&self, l: &Layout) -> Vec<String> {
        let frame = |s: &str| paint(LINE, s);
        let inner = |s: &str| paint(mix(LINE, TRACK, 0.55), s);
        let faint = |s: &str| paint(mix(LINE, TRACK, 0.8), s);
        let sep = format!(" {} ", inner("│"));
        let row = |line: String, band: Option<Rgb>| {
            let inside = format!(" {line} ");
            let inside = band.map_or_else(|| inside.clone(), |b| on_bg(b, &inside));
            format!("  {}{inside}{}", frame("│"), frame("│"))
        };
        let rule = |left: &str, mid: &str, right: &str, line: &str, paint_it: &dyn Fn(&str) -> String| {
            let segs: Vec<String> = l.widths.iter().map(|w| line.repeat(w + 2)).collect();
            format!("  {}{}{}", frame(left), paint_it(&segs.join(mid)), frame(right))
        };
        let mut out = Vec::new();
        if let Some(t) = &self.title {
            out.push(format!("  {}", bold(&paint(ROSE, &format!("✦ {t}")))));
        }
        out.push(rule("┌", "┬", "┐", "─", &frame));
        for line in self.cells(l, &l.header, &Look::heading(), &sep) {
            out.push(row(line, None));
        }
        out.push(rule("├", "┼", "┤", "─", &frame));
        let divided = l.rows.iter().any(|(r, _)| height(r) > 1);
        for (n, (cells, marked)) in l.rows.iter().enumerate() {
            if divided && n > 0 {
                out.push(rule("├", &faint("┼"), "┤", "┄", &faint));
            }
            let band = self.band(n, *marked);
            for line in self.cells(l, cells, &Look::body(*marked), &sep) {
                out.push(row(line, band));
            }
        }
        if let Some(footer) = &l.footer {
            out.push(rule("├", "┼", "┤", "─", &inner));
            for line in self.cells(l, footer, &Look::footer(), &sep) {
                out.push(row(line, None));
            }
        }
        out.push(rule("└", "┴", "┘", "─", &frame));
        out
    }

    /// One table row as lines (several if a column wraps), cells padded and joined by `sep`.
    fn cells(&self, l: &Layout, cells: &[Vec<String>], look: &Look, sep: &str) -> Vec<String> {
        (0..height(cells))
            .map(|i| {
                let parts: Vec<String> = (0..l.cols)
                    .map(|c| {
                        let raw = cells[c].get(i).map_or("", String::as_str);
                        // plain cells get the row's colour; styled ones keep their own; an
                        // empty body cell gets a quiet dot, so it reads as empty, not missing
                        let cell = if raw.contains('\x1b') {
                            raw.to_string()
                        } else if look.body && i == 0 && cells[c].iter().all(String::is_empty) {
                            paint(MUTED, "·")
                        } else if look.bold {
                            bold(&paint(look.color, raw))
                        } else {
                            paint(look.color, raw)
                        };
                        match l.aligns[c] {
                            Align::Left => text::pad(&cell, l.widths[c]),
                            Align::Right => text::pad_left(&cell, l.widths[c]),
                        }
                    })
                    .collect();
                parts.join(sep)
            })
            .collect()
    }

    /// Print the table.
    pub fn print(&self) {
        self.lines().print();
    }
}

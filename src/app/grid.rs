//! A table you can move through: a cursor row, scrolling both ways, the header kept in view.

use std::cell::Cell;

use super::widgets::{centered, scrollbar};
use super::{Event, Frame, Rect, Widget};
use crate::Spinner;
use crate::style::palette::{BERRY, MUTED, RED, ROSE, TRACK};
use crate::style::{ColorMode, Rgb, color_mode, mix, paint, sample};
use crate::table::{Align, Table};
use crate::text;

/// Where the cursor is in a [`Grid`], and how far it's scrolled. Keep one in your app for
/// each grid, move it in [`event`](super::App::event), and hand it to the grid in
/// [`view`](super::App::view).
///
/// ```
/// use tinsel::app::{Cursor, Event, Key};
///
/// let mut cursor = Cursor::new();
/// cursor.navigate(3, &Event::Key(Key::Down));
/// cursor.navigate(3, &Event::Key(Key::End));
/// assert_eq!(cursor.row(), 2);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Cursor {
    row: usize,
    column: usize,
    // what the last drawing found, so the next move knows where it can go
    top: Cell<usize>,
    last_column: Cell<usize>,
}

impl Cursor {
    /// A cursor on the first row, scrolled to the left edge.
    pub fn new() -> Self {
        Cursor::default()
    }

    /// The row the cursor is on, counted from 0.
    pub fn row(&self) -> usize {
        self.row
    }

    /// Put the cursor on `row`.
    pub fn select(&mut self, row: usize) {
        self.row = row;
    }

    /// Move the cursor through `len` rows with the keys [`navigate`](super::navigate) knows.
    /// Returns whether it moved.
    pub fn navigate(&mut self, len: usize, event: &Event) -> bool {
        super::navigate(&mut self.row, len, event)
    }

    /// The first column showing, when the grid is scrolled sideways.
    pub fn column(&self) -> usize {
        self.column
    }

    /// Scroll one column to the right, if anything's cut off there. Returns whether it moved.
    ///
    /// ```
    /// use tinsel::app::{Cursor, Frame, Grid};
    ///
    /// let mut cursor = Cursor::new();
    /// let grid = |c| Grid::new(["name", "env", "owner"]).row(["api", "production", "platform"]).cursor(c);
    /// let mut frame = Frame::new(16, 3);
    /// frame.render(frame.area(), grid(&cursor));
    /// assert!(cursor.scroll_right());
    /// ```
    pub fn scroll_right(&mut self) -> bool {
        let moved = self.column < self.last_column.get();
        if moved {
            self.column += 1;
        }
        moved
    }

    /// Scroll one column back to the left. Returns whether it moved, so a key that scrolls
    /// can do something else (move to another panel, say) once it's at the edge.
    pub fn scroll_left(&mut self) -> bool {
        let moved = self.column > 0;
        self.column = self.column.saturating_sub(1);
        moved
    }
}

#[derive(Clone, Debug)]
enum Body {
    Rows,
    Loading,
    Error(String),
}

/// A [`Table`] with a cursor: rows to move through with ↑↓, the chosen one lit with a bar
/// and a soft band, scrolling down to keep it in view and sideways when the table is wider
/// than its area, the header always on top. Cells are lined up exactly as a printed
/// [`Table`] lines them up: numbers right, words left, styled cells fine.
///
/// ```
/// use tinsel::app::{Cursor, Frame, Grid};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut cursor = Cursor::new();
/// cursor.select(1);
/// let grid = Grid::new(["service", "pods"])
///     .row(["api", "12"])
///     .row(["worker", "3"])
///     .cursor(&cursor);
/// let mut frame = Frame::new(20, 4);
/// frame.render(frame.area(), grid);
/// assert_eq!(frame.snapshot().plain(), "  service   pods\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  api         12\n› worker       3");
/// ```
#[derive(Clone, Debug)]
pub struct Grid<'a> {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    align: Vec<(usize, Align)>,
    limits: Vec<(usize, usize)>,
    cursor: Option<&'a Cursor>,
    focused: bool,
    striped: bool,
    sorted: Option<(usize, bool)>,
    body: Body,
    empty: String,
}

impl<'a> Grid<'a> {
    /// A grid with these column headers.
    pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Self {
        Grid {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
            align: Vec::new(),
            limits: Vec::new(),
            cursor: None,
            focused: true,
            striped: false,
            sorted: None,
            body: Body::Rows,
            empty: "nothing here yet".into(),
        }
    }

    /// Add a row of cells. Styled cells ([`badge`](crate::badge)s, coloured dots) are fine.
    pub fn row<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Override the automatic alignment of one column.
    pub fn align(mut self, column: usize, align: Align) -> Self {
        self.align.push((column, align));
        self
    }

    /// Cut cells in a column to at most `cols` wide, ending in `…`.
    pub fn max_width(mut self, column: usize, cols: usize) -> Self {
        self.limits.push((column, cols.max(1)));
        self
    }

    /// Where the cursor is. Without one, nothing is chosen and the grid starts at the top.
    pub fn cursor(mut self, cursor: &'a Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Whether keys go to this grid. When they don't, the cursor row stays marked, but
    /// quietly. Focused unless set.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Tint every other row faintly, to help the eye along wide rows.
    pub fn striped(mut self) -> Self {
        self.striped = true;
        self
    }

    /// Mark `column` as the one the rows are sorted by, smallest first: `▲` beside its
    /// header. Sort the rows yourself, so the cursor's row is the one you think it is.
    pub fn sorted(mut self, column: usize) -> Self {
        self.sorted = Some((column, false));
        self
    }

    /// Like [`sorted`](Grid::sorted), largest first: `▼`.
    pub fn sorted_desc(mut self, column: usize) -> Self {
        self.sorted = Some((column, true));
        self
    }

    /// Show that the rows are still on their way: shimmering placeholder rows under the
    /// header, and a spinner.
    pub fn loading(mut self, loading: bool) -> Self {
        if loading {
            self.body = Body::Loading;
        }
        self
    }

    /// Show that the rows couldn't be had, instead of any rows.
    pub fn error(mut self, message: impl Into<String>) -> Self {
        self.body = Body::Error(message.into());
        self
    }

    /// What to say when there are no rows. "nothing here yet" unless set.
    pub fn empty(mut self, message: impl Into<String>) -> Self {
        self.empty = message.into();
        self
    }

    /// Which row is at `x`, `y`, if the grid is drawn at `area`: for clicks.
    pub fn row_at(&self, area: Rect, x: usize, y: usize) -> Option<usize> {
        if !area.contains(x, y) || y < area.y + 2 || !matches!(self.body, Body::Rows) {
            return None;
        }
        let top = self.cursor.map_or(0, |c| c.top.get());
        let i = top + y - area.y - 2;
        (i < self.rows.len()).then_some(i)
    }

    fn headers(&self) -> Vec<String> {
        let mut headers = self.headers.clone();
        if let Some((col, desc)) = self.sorted {
            if let Some(h) = headers.get_mut(col) {
                h.push_str(if desc { " ▼" } else { " ▲" });
            }
        }
        headers
    }

    fn table(&self, headers: &[String]) -> Table {
        let mut table = Table::new(headers.iter().cloned());
        for row in &self.rows {
            table = table.row(row.iter().cloned());
        }
        for &(col, align) in &self.align {
            table = table.align(col, align);
        }
        for &(col, cols) in &self.limits {
            table = table.max_width(col, cols);
        }
        table
    }

    /// How wide each column is, as the table lays them out.
    fn widths(&self, headers: &[String]) -> Vec<usize> {
        let cols = self.rows.iter().map(Vec::len).chain([headers.len()]).max().unwrap_or(0);
        (0..cols)
            .map(|c| {
                let limit = self.limits.iter().rev().find(|l| l.0 == c).map(|l| l.1);
                self.rows
                    .iter()
                    .chain([&headers.to_vec()])
                    .filter_map(|r| r.get(c))
                    .map(|cell| {
                        let w = text::width(cell);
                        limit.map_or(w, |l| w.min(l))
                    })
                    .max()
                    .unwrap_or(0)
                    .max(1)
            })
            .collect()
    }
}

/// The band behind the cursor row.
fn band(focused: bool) -> Rgb {
    mix(TRACK, BERRY, if focused { 0.35 } else { 0.16 })
}

const STRIPE: Rgb = Rgb(38, 32, 36);

impl Widget for Grid<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        let headers = self.headers();
        let lines = self.table(&headers).lines().into_lines();
        // every line starts with the table's two-column margin, which the gutter replaces
        let line = |i: usize| lines.get(i).map(|l| l.chars().skip(2).collect::<String>());

        let n = self.rows.len();
        let rows = matches!(self.body, Body::Rows) && n > 0;
        let body = Rect::new(area.x, area.y + 2, area.width, area.height.saturating_sub(2));
        let chosen = self.cursor.filter(|_| rows).map(|c| c.row.min(n - 1));
        let overflow = rows && n > body.height;
        let content = Rect::new(
            area.x + 2,
            area.y,
            area.width.saturating_sub(2 + overflow as usize),
            area.height,
        );

        // sideways: scroll by whole columns, but never past the right edge of the last one
        let widths = self.widths(&headers);
        let inner = widths.iter().sum::<usize>() + 3 * widths.len().saturating_sub(1);
        let furthest = inner.saturating_sub(content.width);
        let starts: Vec<usize> = widths
            .iter()
            .scan(0, |at, w| {
                let start = *at;
                *at += w + 3;
                Some(start)
            })
            .collect();
        let last_column = starts.iter().position(|&s| s >= furthest).unwrap_or(0);
        let column = self.cursor.map_or(0, |c| c.column.min(last_column));
        let skip = starts.get(column).map_or(0, |&s| s.min(furthest));

        let mut top = self.cursor.map_or(0, |c| c.top.get());
        if let Some(chosen) = chosen {
            if chosen < top {
                top = chosen;
            } else if chosen >= top + body.height {
                top = chosen + 1 - body.height;
            }
        }
        top = top.min(n.saturating_sub(body.height));
        if let Some(c) = self.cursor {
            c.top.set(top);
            c.last_column.set(last_column);
        }

        for (row, i) in [(0, 0), (1, 1)] {
            if let Some(l) = line(i) {
                frame.print_from(content, row, &l, skip);
            }
        }

        match &self.body {
            Body::Error(message) => {
                let message = format!("{}  {}", paint(RED, "✕"), paint(RED, message));
                centered(frame, body, &message);
            }
            Body::Loading => skeleton(frame, body, content, &widths, skip),
            Body::Rows if n == 0 => centered(frame, body, &paint(MUTED, &self.empty)),
            Body::Rows => {
                let across = Rect::new(area.x, 0, area.width - overflow as usize, 1);
                for (row, i) in (top..n).take(body.height).enumerate() {
                    let y = body.y + row;
                    let strip = Rect { y, ..across };
                    if Some(i) == chosen {
                        frame.tint(strip, band(self.focused));
                        let mark = if color_mode() == ColorMode::None { "›" } else { "▌" };
                        let color = if self.focused { ROSE } else { MUTED };
                        frame.print(strip, 0, &paint(color, mark));
                    } else if self.striped && i % 2 == 1 {
                        frame.tint(strip, STRIPE);
                    }
                    if let Some(l) = line(i + 2) {
                        frame.print_from(Rect { y, ..content }, 0, &l, skip);
                    }
                }
                if overflow {
                    let bar = Rect::new(area.x + area.width - 1, body.y, 1, body.height);
                    scrollbar(frame, bar, top, n);
                }
            }
        }
    }
}

/// Placeholder rows while the real ones load: a bar per column, catching a light that
/// sweeps across, with a spinner in the middle.
fn skeleton(frame: &mut Frame, body: Rect, content: Rect, widths: &[usize], skip: usize) {
    let t = frame.time();
    if color_mode() != ColorMode::None {
        let sweep = (t * 0.8).rem_euclid(1.6) - 0.3;
        for row in 0..body.height.min(8) {
            let mut line = String::new();
            let mut col: usize = 0;
            for (c, w) in widths.iter().enumerate() {
                // each bar a little shorter or longer, so they read as text to come
                let seed = crate::widgets::noise((row * 31 + c) as u32, 7);
                let len = ((*w as f32) * (0.45 + 0.55 * seed)).round().max(1.0) as usize;
                for x in 0..*w {
                    let at = col.saturating_sub(skip) as f32 / content.width.max(1) as f32;
                    col += 1;
                    let glow = (1.0f32 - ((at - sweep).abs() * 4.0)).max(0.0);
                    let ink = sample(&[mix(TRACK, BERRY, 0.25), mix(TRACK, ROSE, 0.6)], glow);
                    line.push_str(&if x < len { paint(ink, "━") } else { " ".into() });
                }
                line.push_str("   ");
                col += 3;
            }
            frame.print_from(
                Rect {
                    y: body.y + row,
                    ..content
                },
                0,
                &line,
                skip,
            );
        }
    }
    let spinner = Spinner::Dots.frame(t);
    centered(frame, body, &format!("{spinner} {}", paint(MUTED, "loading…")));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(w: usize, h: usize, grid: Grid) -> String {
        crate::set_color_mode(ColorMode::None);
        let mut f = Frame::new(w, h);
        f.render(f.area(), grid);
        f.snapshot().plain()
    }

    #[test]
    fn widths_follow_limits_and_sort_marks() {
        let grid = Grid::new(["a", "bb"]).row(["long cell", "x"]).max_width(0, 4).sorted(1);
        assert_eq!(grid.widths(&grid.headers()), [4, 4]);
        assert_eq!(drawn(14, 3, grid), "  a      bb ▲\n  ┈┈┈┈┈┈┈┈┈┈┈\n  lon…   x");
    }

    #[test]
    fn states_instead_of_rows() {
        let shown = drawn(20, 5, Grid::new(["name"]).error("couldn't read"));
        assert_eq!(shown.lines().nth(3), Some("  ✕  couldn't read"));
        let shown = drawn(20, 5, Grid::new(["name"]));
        assert!(shown.contains("nothing here yet"), "{shown}");
        let shown = drawn(20, 5, Grid::new(["name"]).row(["x"]).loading(true));
        assert!(shown.contains("loading…") && !shown.contains('x'), "{shown}");
    }

    #[test]
    fn rows_are_found_by_where_they_are() {
        let mut cursor = Cursor::new();
        cursor.select(9);
        let rows: Vec<String> = (0..10).map(|i| i.to_string()).collect();
        let grid = rows
            .iter()
            .fold(Grid::new(["n"]), |g, r| g.row([r.as_str()]))
            .cursor(&cursor);
        let area = Rect::new(0, 0, 10, 5);
        drawn(10, 5, grid.clone());
        assert_eq!(cursor.top.get(), 7);
        assert_eq!(grid.row_at(area, 3, 2), Some(7));
        assert_eq!(grid.row_at(area, 3, 1), None);
        assert_eq!(grid.row_at(area, 30, 2), None);
    }
}

//! A table you can move through: a cursor row, scrolling both ways, the header kept in view.

use std::cell::Cell;

use super::widgets::{centered, scrollbar};
use super::{Event, Frame, Rect, Widget};
use crate::Spinner;
use crate::style::palette::{BERRY, MUTED, RED, ROSE, TRACK};
use crate::style::{ColorMode, Rgb, bold, color_mode, mix, paint, sample};
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

/// One line of a grid's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line<'g> {
    Heading(&'g str),
    Row(usize),
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
/// [`Table`] lines them up: numbers right, words left, styled cells fine. Rows can be
/// gathered under [`heading`](Grid::heading)s.
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
    /// Group headings: (the row each goes above, its text), in the order they were added.
    headings: Vec<(usize, String)>,
    align: Vec<(usize, Align)>,
    limits: Vec<(usize, usize)>,
    cursor: Option<&'a Cursor>,
    focused: bool,
    striped: bool,
    blank: bool,
    sorted: Option<(usize, bool)>,
    body: Body,
    empty: String,
    fill: Option<usize>,
    /// Spaces added after one column's header, to widen it: (column, spaces).
    grow: Option<(usize, usize)>,
}

impl<'a> Grid<'a> {
    /// A grid with these column headers.
    pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Self {
        Grid {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
            headings: Vec::new(),
            align: Vec::new(),
            limits: Vec::new(),
            cursor: None,
            focused: true,
            striped: false,
            blank: false,
            sorted: None,
            body: Body::Rows,
            empty: "nothing here yet".into(),
            fill: None,
            grow: None,
        }
    }

    /// Give `column` whatever width the grid has to spare, so the grid fills its area
    /// instead of leaving a gap at the right. Longer cells there are cut only when the
    /// area is too narrow.
    pub fn fill(mut self, column: usize) -> Self {
        self.fill = Some(column);
        self
    }

    /// Add a row of cells. Styled cells ([`badge`](crate::badge)s, coloured dots) are fine.
    pub fn row<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Start a group: a heading drawn on its own line above the rows added after it, up to
    /// the next heading. Headings aren't rows: the cursor, [`row_at`](Grid::row_at) and
    /// [`Cursor::navigate`]'s `len` count only the rows, so row 0 is the first row under the
    /// first heading, and moving steps over headings. Scrolling up to a group's first rows
    /// brings its heading into view too.
    ///
    /// ```
    /// use tinsel::app::{Cursor, Frame, Grid};
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let mut cursor = Cursor::new();
    /// cursor.select(1);
    /// let grid = Grid::new(["service", "pods"])
    ///     .heading("frontend")
    ///     .row(["web", "4"])
    ///     .heading("backend")
    ///     .row(["api", "12"])
    ///     .cursor(&cursor);
    /// let mut frame = Frame::new(20, 6);
    /// frame.render(frame.area(), grid);
    /// assert_eq!(
    ///     frame.snapshot().plain(),
    ///     "  service   pods\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  frontend\n  web          4\n  backend\n› api         12"
    /// );
    /// ```
    pub fn heading(mut self, text: impl Into<String>) -> Self {
        self.headings.push((self.rows.len(), text.into()));
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

    /// Leave empty cells blank, instead of marking each with a muted `·`
    /// (see [`Table::blank_cells`]).
    pub fn blank_cells(mut self) -> Self {
        self.blank = true;
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
        match self.lines().get(top + y - area.y - 2) {
            Some(Line::Row(i)) => Some(*i),
            _ => None,
        }
    }

    /// The body's lines from the top: headings and rows, in order.
    fn lines(&self) -> Vec<Line<'_>> {
        let mut headings = self.headings.iter().peekable();
        let mut lines = Vec::with_capacity(self.rows.len() + self.headings.len());
        for i in 0..=self.rows.len() {
            while let Some((_, text)) = headings.next_if(|h| h.0 <= i) {
                lines.push(Line::Heading(text));
            }
            if i < self.rows.len() {
                lines.push(Line::Row(i));
            }
        }
        lines
    }

    /// Which line row `row` is on, counting headings.
    fn line_of(&self, row: usize) -> usize {
        row + self.headings.iter().filter(|h| h.0 <= row).count()
    }

    /// Which line the heading over row `row` is on, if it has one.
    fn heading_of(&self, row: usize) -> Option<usize> {
        // heading k sits after the k headings and the rows before it
        let k = self.headings.iter().rposition(|h| h.0 <= row)?;
        Some(self.headings[k].0 + k)
    }

    fn headers(&self) -> Vec<String> {
        let mut headers = self.headers.clone();
        if let Some((col, desc)) = self.sorted {
            if let Some(h) = headers.get_mut(col) {
                h.push_str(if desc { " ▼" } else { " ▲" });
            }
        }
        if let Some((col, spaces)) = self.grow {
            // a column of cells can be wider than the headers go
            headers.resize(headers.len().max(col + 1), String::new());
            headers[col].push_str(&" ".repeat(spaces));
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
        if self.blank {
            table = table.blank_cells();
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

impl<'a> Grid<'a> {
    /// This grid with its [`fill`](Grid::fill) column widened to fill `area`, if it has one
    /// and there's room to spare.
    fn filled(&self, area: Rect) -> Option<Grid<'a>> {
        let column = self.fill?;
        let headers = self.headers();
        let widths = self.widths(&headers);
        let width = *widths.get(column)?;
        let inner = widths.iter().sum::<usize>() + 3 * widths.len().saturating_sub(1);
        let lines = self.rows.len() + self.headings.len();
        let overflow = matches!(self.body, Body::Rows) && lines > area.height.saturating_sub(2);
        let room = area.width.saturating_sub(2 + overflow as usize);
        let spare = room.checked_sub(inner).filter(|&s| s > 0)?;
        let mut grid = self.clone();
        grid.limits.push((column, width + spare));
        let header = headers.get(column).map_or(0, |h| text::width(h));
        grid.grow = Some((column, width + spare - header));
        Some(grid)
    }
}

impl Widget for Grid<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        if let Some(grid) = self.filled(area) {
            return grid.draw(area, frame);
        }
        self.draw(area, frame);
    }

    /// Every column in full, and every row.
    fn size(&self) -> Option<(usize, usize)> {
        let widths = self.widths(&self.headers());
        let inner = widths.iter().sum::<usize>() + 3 * widths.len().saturating_sub(1);
        let headings = self.headings.iter().map(|h| text::width(&h.1)).max().unwrap_or(0);
        let lines = self.rows.len() + self.headings.len();
        Some((inner.max(headings) + 2, lines.max(1) + 2))
    }
}

impl Grid<'_> {
    fn draw(&self, area: Rect, frame: &mut Frame) {
        let headers = self.headers();
        let lines = self.table(&headers).lines().into_lines();
        // every line starts with the table's two-column margin, which the gutter replaces
        let line = |i: usize| lines.get(i).map(|l| l.chars().skip(2).collect::<String>());

        let n = self.rows.len();
        let rows = matches!(self.body, Body::Rows) && n > 0;
        let body = Rect::new(area.x, area.y + 2, area.width, area.height.saturating_sub(2));
        let chosen = self.cursor.filter(|_| rows).map(|c| c.row.min(n - 1));
        let body_lines = self.lines();
        let total = body_lines.len();
        let overflow = rows && total > body.height;
        let content = Rect::new(
            area.x + 2,
            area.y,
            area.width.saturating_sub(2 + overflow as usize),
            area.height,
        );

        // sideways: a whole column at a time, until everything from there on fits
        let widths = self.widths(&headers);
        let inner = widths.iter().sum::<usize>() + 3 * widths.len().saturating_sub(1);
        let starts: Vec<usize> = widths
            .iter()
            .scan(0, |at, w| {
                let start = *at;
                *at += w + 3;
                Some(start)
            })
            .collect();
        let last_column = starts
            .iter()
            .position(|&s| inner - s <= content.width)
            .unwrap_or(starts.len().saturating_sub(1));
        let column = self.cursor.map_or(0, |c| c.column.min(last_column));
        let skip = starts.get(column).copied().unwrap_or(0);

        let mut top = self.cursor.map_or(0, |c| c.top.get());
        if let Some(chosen) = chosen {
            let at = self.line_of(chosen);
            if at < top {
                top = at;
            } else if at >= top + body.height {
                top = at + 1 - body.height;
            }
            // show the row's heading too, if it fits on screen with the row
            if let Some(heading) = self.heading_of(chosen) {
                if heading < top && at - heading < body.height {
                    top = heading;
                }
            }
        }
        top = top.min(total.saturating_sub(body.height));
        if let Some(c) = self.cursor {
            c.top.set(top);
            c.last_column.set(last_column);
        }

        // the header and the rule under it
        for row in 0..2 {
            frame.print_from(content, row, &line(row).unwrap_or_default(), skip);
        }
        // where there's more to either side, an arrow at that end of the rule says so
        if area.height > 1 && content.width > 0 {
            if skip > 0 {
                frame.print(Rect::new(area.x + 1, area.y + 1, 1, 1), 0, &paint(ROSE, "‹"));
            }
            if inner - skip > content.width {
                let right = content.x + content.width - 1;
                frame.print(Rect::new(right, area.y + 1, 1, 1), 0, &paint(ROSE, "›"));
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
                for (row, &item) in body_lines.iter().skip(top).take(body.height).enumerate() {
                    let y = body.y + row;
                    let strip = Rect { y, ..across };
                    let i = match item {
                        Line::Row(i) => i,
                        Line::Heading(heading) => {
                            // headings stay put when the columns scroll sideways
                            let fitted = if text::width(heading) > content.width {
                                text::truncate(&text::strip_ansi(heading), content.width)
                            } else {
                                heading.to_string()
                            };
                            frame.print(Rect { y, ..content }, 0, &bold(&paint(ROSE, &fitted)));
                            continue;
                        }
                    };
                    if Some(i) == chosen {
                        frame.tint(strip, band(self.focused));
                        let mark = if color_mode() == ColorMode::None { "›" } else { "▌" };
                        let color = if self.focused { ROSE } else { MUTED };
                        frame.print(strip, 0, &paint(color, mark));
                    } else if self.striped && i % 2 == 1 {
                        frame.tint(strip, STRIPE);
                    }
                    let l = line(i + 2).unwrap_or_default();
                    frame.print_from(Rect { y, ..content }, 0, &l, skip);
                }
                if overflow {
                    let bar = Rect::new(area.x + area.width - 1, body.y, 1, body.height);
                    scrollbar(frame, bar, top, total);
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
        assert_eq!(grid.size(), Some((13, 3)));
        assert_eq!(drawn(14, 3, grid), "  a      bb ▲\n  ┈┈┈┈┈┈┈┈┈┈┈\n  lon…   x");
    }

    #[test]
    fn a_fill_column_takes_the_spare_width() {
        let grid = || {
            Grid::new(["name", "n"])
                .row(["a-long-name", "1"])
                .max_width(0, 6)
                .fill(0)
        };
        // room to spare: the name column grows, so nothing's cut and the rule spans the area
        assert_eq!(
            drawn(24, 3, grid()),
            "  name                 n\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  a-long-name          1"
        );
        // no room: as without fill
        assert_eq!(drawn(12, 3, grid()), "  name     n\n  ┈┈┈┈┈┈┈┈┈┈\n  a-lon…   1");
    }

    #[test]
    fn empty_cells_can_be_blank() {
        let grid = || Grid::new(["name", "owner"]).row(["api", ""]).row(["web", "ops"]);
        assert_eq!(
            drawn(16, 4, grid()),
            "  name   owner\n  ┈┈┈┈┈┈┈┈┈┈┈┈\n  api    ·\n  web    ops"
        );
        assert_eq!(
            drawn(16, 4, grid().blank_cells()),
            "  name   owner\n  ┈┈┈┈┈┈┈┈┈┈┈┈\n  api\n  web    ops"
        );
    }

    /// Two groups of three rows, under headings "a" and "b", rows named r0 to r5.
    fn grouped(cursor: &Cursor) -> Grid<'_> {
        let mut grid = Grid::new(["name"]).cursor(cursor);
        for i in 0..6 {
            if i % 3 == 0 {
                grid = grid.heading(if i == 0 { "a" } else { "b" });
            }
            grid = grid.row([format!("r{i}")]);
        }
        grid
    }

    #[test]
    fn headings_sit_between_rows_and_the_cursor_counts_rows_only() {
        let mut cursor = Cursor::new();
        cursor.select(3);
        let shown = drawn(12, 10, grouped(&cursor));
        assert_eq!(shown, "  name\n  ┈┈┈┈\n  a\n  r0\n  r1\n  r2\n  b\n› r3\n  r4\n  r5");
        // moving steps from the last row of one group to the first of the next
        cursor.select(2);
        assert!(cursor.navigate(6, &Event::Key(super::super::Key::Down)));
        assert_eq!(cursor.row(), 3);
        assert_eq!(grouped(&cursor).size(), Some((6, 10)));
    }

    #[test]
    fn scrolling_keeps_the_cursor_row_and_its_heading_in_view() {
        let mut cursor = Cursor::new();
        let mut at = |row: usize| {
            cursor.select(row);
            let shown = drawn(12, 5, grouped(&cursor));
            let body: Vec<String> = shown
                .lines()
                .skip(2)
                .map(|l| l.trim_end_matches(['│', '┃']).trim().to_string())
                .collect();
            (body.join(" "), cursor.top.get())
        };
        assert_eq!(at(0), ("a › r0 r1".into(), 0));
        // down into the next group: its heading comes along
        assert_eq!(at(3), ("r2 b › r3".into(), 3));
        assert_eq!(at(5), ("r3 r4 › r5".into(), 5));
        // back up to a group's first row: its heading shows again
        assert_eq!(at(3), ("b › r3 r4".into(), 4));
        // too far from its heading to show both: just the row
        assert_eq!(at(2), ("› r2 b r3".into(), 3));
        assert_eq!(at(1), ("a r0 › r1".into(), 0));
    }

    #[test]
    fn clicks_on_headings_find_no_row() {
        let mut cursor = Cursor::new();
        cursor.select(5);
        let area = Rect::new(0, 0, 12, 5);
        drawn(12, 5, grouped(&cursor));
        cursor.select(3);
        drawn(12, 5, grouped(&cursor));
        let grid = grouped(&cursor);
        assert_eq!(cursor.top.get(), 4);
        assert_eq!(grid.row_at(area, 3, 2), None, "the heading");
        assert_eq!(grid.row_at(area, 3, 3), Some(3));
        assert_eq!(grid.row_at(area, 3, 4), Some(4));
        let flat = Grid::new(["n"]).heading("only").row(["x"]);
        assert_eq!(flat.row_at(area, 3, 2), None);
        assert_eq!(flat.row_at(area, 3, 3), Some(0));
        assert_eq!(flat.row_at(area, 3, 4), None);
    }

    #[test]
    fn headings_work_with_fill_and_sideways_scrolling() {
        let grid = Grid::new(["name", "n"])
            .heading("group")
            .row(["a-long-name", "1"])
            .max_width(0, 6)
            .fill(0);
        assert_eq!(
            drawn(24, 4, grid),
            "  name                 n\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  group\n  a-long-name          1"
        );
        // the heading counts towards overflowing, so the scrollbar's column is kept free
        let grid = Grid::new(["name", "n"]).heading("group").row(["abc", "1"]).fill(0);
        assert_eq!(drawn(12, 3, grid), "  name    n\n  ┈┈┈┈┈┈┈┈┈\n  group    ┃");

        let mut cursor = Cursor::new();
        fn services(c: &Cursor) -> Grid<'_> {
            Grid::new(["name", "env", "owner"])
                .heading("services")
                .row(["api", "production", "platform"])
                .cursor(c)
        }
        drawn(16, 4, services(&cursor));
        assert!(cursor.scroll_right());
        let shown = drawn(16, 4, services(&cursor));
        assert_eq!(shown.lines().nth(2), Some("  services"), "{shown}");
        assert!(
            shown
                .lines()
                .nth(3)
                .is_some_and(|l| l.contains("production") && !l.contains("api")),
            "{shown}"
        );
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

    #[test]
    fn a_fill_column_without_a_header_still_fills() {
        let grid = Grid::new(["name"]).row(["api", "extra"]).fill(1);
        assert_eq!(drawn(20, 3, grid), "  name\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  api    extra");
    }

    #[test]
    fn alignment_sort_marks_and_messages() {
        let grid = Grid::new(["n", "name"])
            .row(["1", "a"])
            .align(0, Align::Left)
            .sorted_desc(0);
        assert_eq!(drawn(12, 3, grid), "  n ▼   name\n  ┈┈┈┈┈┈┈┈┈┈\n  1     a");
        let none = Grid::new(["name"]).empty("no services").loading(false).sorted(5);
        assert!(drawn(20, 3, none).ends_with("no services"));
        drawn(0, 0, Grid::new(["x"]));
    }

    #[test]
    fn long_headings_are_cut_to_fit() {
        let grid = Grid::new(["n"]).heading("a heading much too long").row(["1"]);
        let shown = drawn(10, 4, grid);
        assert!(shown.lines().nth(2).unwrap().ends_with('…'), "{shown}");
    }

    #[test]
    fn a_heading_too_far_above_stays_off_screen() {
        let mut cursor = Cursor::new();
        cursor.select(7);
        let mut grid = Grid::new(["name"]).heading("far").cursor(&cursor);
        for i in 0..8 {
            grid = grid.row([format!("r{i}")]);
        }
        let shown = drawn(10, 5, grid);
        assert!(!shown.contains("far") && shown.contains("r7"), "{shown}");
    }

    #[test]
    fn loading_rows_shimmer_in_colour() {
        let grid = Grid::new(["name", "status"]).loading(true);
        let mut f = Frame::new(30, 6).at(0.5);
        crate::style::with_color_mode(ColorMode::TrueColor, || f.render(f.area(), grid));
        assert!(f.snapshot().plain().contains('━'));
    }

    #[test]
    fn a_grid_one_row_tall_shows_just_its_header() {
        let grid = Grid::new(["name", "status"]).row(["api", "up"]);
        assert_eq!(drawn(4, 1, grid), "  n");
    }

    #[test]
    fn striped_rows_alternate() {
        let cursor = Cursor::new();
        let grid = Grid::new(["n"])
            .row(["a"])
            .row(["b"])
            .row(["c"])
            .striped()
            .cursor(&cursor);
        let mut f = Frame::new(10, 5);
        crate::style::with_color_mode(ColorMode::TrueColor, || f.render(f.area(), grid));
        assert_ne!(f.snapshot().lines()[3], f.snapshot().lines()[4]);
    }
}

//! Things to put on the screen: panels, lists, tabs, charts and stats, all in the same pink
//! as the rest of tinsel. Anything tinsel already draws ([`Block`], [`Table`], [`Card`], …)
//! can go straight into a panel too.

use super::{Frame, Rect};
use crate::block::Block;
use crate::layout::Code;
use crate::style::palette::{BERRY, BLUSH, LINE, MUTED, PETAL, ROSE, SHINE, SOFT, TEXT, TRACK};
use crate::style::{Line, RAMP, Rgb, bold, gradient, mix, paint, paint_bg, sample};
use crate::table::Table;
use crate::text;
use crate::tree::Tree;
use crate::widgets::{Card, noise};

/// Something that can draw itself into a rectangle of a [`Frame`].
///
/// Implement it for your own pieces; they can then go anywhere a built-in one can, including
/// inside a [`Panel`].
///
/// ```
/// use tinsel::app::{Frame, Rect, Widget};
/// use tinsel::{paint, palette};
///
/// struct Heart;
///
/// impl Widget for Heart {
///     fn render(&self, area: Rect, frame: &mut Frame) {
///         let heart = paint(palette::ROSE, "♥");
///         for row in 0..area.height {
///             frame.print(area, row, &heart.repeat(area.width));
///         }
///     }
/// }
/// ```
pub trait Widget {
    /// Draw into `area` of `frame`. `area` is already clipped to the screen.
    fn render(&self, area: Rect, frame: &mut Frame);

    /// How many columns and rows it would like, if it knows: what a [`Modal`](super::Modal)
    /// is sized to. `None` unless implemented.
    ///
    /// ```
    /// use tinsel::app::{Panel, Widget};
    ///
    /// assert_eq!("two\nlines".size(), Some((5, 2)));
    /// assert_eq!(Panel::new("hi").body("two\nlines").size(), Some((9, 4)));
    /// ```
    fn size(&self) -> Option<(usize, usize)> {
        None
    }
}

impl<W: Widget + ?Sized> Widget for &W {
    fn render(&self, area: Rect, frame: &mut Frame) {
        (**self).render(area, frame)
    }

    fn size(&self) -> Option<(usize, usize)> {
        (**self).size()
    }
}

impl<W: Widget + ?Sized> Widget for Box<W> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        (**self).render(area, frame)
    }

    fn size(&self) -> Option<(usize, usize)> {
        (**self).size()
    }
}

/// Text is word-wrapped to fit. Styled text is fine: colours carry on across wrapped lines.
impl Widget for str {
    fn render(&self, area: Rect, frame: &mut Frame) {
        for (row, line) in wrap(self, area.width).into_iter().take(area.height).enumerate() {
            frame.print(area, row, &line);
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let lines = self.split('\n');
        Some((lines.clone().map(text::width).max().unwrap_or(0), lines.count()))
    }
}

/// Word-wrap styled text to `cols` columns. Words wider than a line are broken by columns
/// (never inside an escape code), and each line starts by restating whatever styling was
/// still on at the end of the one before, since every row is drawn from a clean slate.
pub(super) fn wrap(s: &str, cols: usize) -> Vec<String> {
    let mut w = Wrap {
        cols: cols.max(1),
        ..Wrap::default()
    };
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                let mut seq = String::from(c);
                match chars.next() {
                    Some('[') => {
                        seq.push('[');
                        for c in chars.by_ref() {
                            seq.push(c);
                            if ('@'..='~').contains(&c) {
                                break;
                            }
                        }
                    }
                    Some(']') => {
                        seq.push(']');
                        while let Some(c) = chars.next() {
                            seq.push(c);
                            if c == '\x07' {
                                break;
                            }
                            if c == '\x1b' && chars.next_if_eq(&'\\').is_some() {
                                seq.push('\\');
                                break;
                            }
                        }
                    }
                    Some(c) => seq.push(c),
                    None => {}
                }
                w.escape(&seq);
            }
            '\n' => {
                w.end_word();
                w.end_line();
                w.line.clone_from(&w.style);
            }
            ' ' => {
                w.end_word();
                w.space = true;
            }
            c => w.char(c),
        }
    }
    w.end_word();
    w.end_line();
    w.lines
}

#[derive(Default)]
struct Wrap {
    cols: usize,
    lines: Vec<String>,
    line: String,
    line_w: usize,
    word: String,
    word_w: usize,
    /// The colour codes in effect now, to restate at the start of a line.
    style: String,
    /// The same, as they were when the current word began.
    word_style: String,
    /// Whether a space goes before the next word.
    space: bool,
}

impl Wrap {
    fn begin_word(&mut self) {
        if self.word.is_empty() {
            self.word_style.clone_from(&self.style);
        }
    }

    fn escape(&mut self, seq: &str) {
        self.begin_word();
        self.word.push_str(seq);
        if let Some(params) = seq.strip_prefix("\x1b[").and_then(|p| p.strip_suffix('m')) {
            if params.is_empty() || params == "0" {
                self.style.clear();
            } else {
                self.style.push_str(seq);
            }
        }
    }

    fn char(&mut self, c: char) {
        let cw = text::char_width(c);
        if self.word_w > 0 && self.word_w + cw > self.cols {
            // a word wider than a line: break it here
            self.end_word();
        }
        self.begin_word();
        self.word.push(c);
        self.word_w += cw;
    }

    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        if self.word_w > 0 {
            let gap = (self.space && self.line_w > 0) as usize;
            if self.line_w > 0 && self.line_w + gap + self.word_w > self.cols {
                self.end_line();
                self.line.clone_from(&self.word_style);
            } else if gap == 1 {
                self.line.push(' ');
                self.line_w += 1;
            }
            self.space = false;
        }
        self.line.push_str(&self.word);
        self.line_w += self.word_w;
        self.word.clear();
        self.word_w = 0;
    }

    fn end_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
        self.line_w = 0;
        self.space = false;
    }
}

impl Widget for String {
    fn render(&self, area: Rect, frame: &mut Frame) {
        self.as_str().render(area, frame)
    }

    fn size(&self) -> Option<(usize, usize)> {
        self.as_str().size()
    }
}

impl Widget for Line {
    fn render(&self, area: Rect, frame: &mut Frame) {
        frame.print(area, 0, &self.to_string());
    }

    fn size(&self) -> Option<(usize, usize)> {
        Some((self.width(), 1))
    }
}

/// Blocks are drawn a line per row, cut off rather than wrapped. The two-space margin tinsel
/// gives printed output is taken off, since the block is placed exactly where it's wanted.
impl Widget for Block {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let margin = margin(self);
        for (row, line) in self.lines().iter().take(area.height).enumerate() {
            frame.print(area, row, &drop_columns(line, margin));
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let margin = margin(self);
        let widest = self.lines().iter().map(|l| text::width(l)).max().unwrap_or(0);
        Some((widest.saturating_sub(margin), self.len()))
    }
}

/// The spaces every non-blank line of a block starts with.
fn margin(block: &Block) -> usize {
    block
        .lines()
        .iter()
        .map(|l| text::strip_ansi(l))
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0)
}

/// The line without its first `n` columns, which are spaces, keeping any styling around them.
fn drop_columns(line: &str, mut n: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while n > 0 {
        match chars.next() {
            Some(' ') => n -= 1,
            Some('\x1b') => {
                out.push('\x1b');
                for c in chars.by_ref() {
                    out.push(c);
                    if c != '[' && ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(c) => {
                out.push(c);
                break;
            }
            None => break,
        }
    }
    out.extend(chars);
    out
}

/// Cut styled text to at most `cols` columns, ending in "…" when anything was cut. Escape
/// codes are kept whole, and styling is reset after the "…" so it doesn't run on.
fn clip(s: &str, cols: usize) -> String {
    if text::width(s) <= cols {
        return s.to_string();
    }
    if !s.contains('\x1b') {
        return text::truncate(s, cols);
    }
    let mut out = String::with_capacity(s.len());
    let mut used = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            out.push(c);
            for c in chars.by_ref() {
                out.push(c);
                if c != '[' && ('@'..='~').contains(&c) {
                    break;
                }
            }
            continue;
        }
        let w = text::char_width(c);
        if used + w + 1 > cols {
            break;
        }
        out.push(c);
        used += w;
    }
    if cols > 0 {
        out.push('…');
    }
    out + "\x1b[0m"
}

macro_rules! via_lines {
    ($($ty:ty),*) => {$(
        impl Widget for $ty {
            fn render(&self, area: Rect, frame: &mut Frame) {
                self.lines().render(area, frame)
            }

            fn size(&self) -> Option<(usize, usize)> {
                self.lines().size()
            }
        }
    )*};
}

via_lines!(Table, Tree, Code);

/// Cards fill the width they're given (unless they were given one of their own), without the
/// blank line above them that printing adds.
impl Widget for Card {
    fn render(&self, area: Rect, frame: &mut Frame) {
        frame.lines(area, &self.boxed(self.fixed_width().unwrap_or(area.width)));
    }

    fn size(&self) -> Option<(usize, usize)> {
        let w = self.fixed_width()?;
        Some((w, self.boxed(w).len()))
    }
}

// MARK: - Panel

/// A box with a rounded border and a title, holding any other widget. A focused panel's
/// border shimmers, so it's clear where keys will go.
///
/// Along the top border go the title, a [`count`](Panel::count) beside it and a
/// [`status`](Panel::status) at the right; along the bottom, a [`footer`](Panel::footer).
/// When the panel is too narrow for them all, the status is cut short and then left off,
/// then the count goes, and last the title is cut short. The border itself is never broken.
///
/// ```
/// use tinsel::app::{Frame, Panel};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(24, 4);
/// frame.render(frame.area(), Panel::new("notes").body("buy glitter").footer("1/3"));
/// assert_eq!(
///     frame.snapshot().plain(),
///     "╭─ notes ──────────────╮\n│ buy glitter          │\n│                      │\n╰──────────────── 1/3 ─╯"
/// );
/// ```
pub struct Panel<'a> {
    title: String,
    count: String,
    status: String,
    footer: String,
    body: Option<Box<dyn Widget + 'a>>,
    focused: bool,
    padding: (usize, usize),
}

impl<'a> Panel<'a> {
    /// A panel with this title (which can be empty). Plain text is drawn in the panel's own
    /// quiet colour (pink when focused); styled text keeps its own colours, so a title can
    /// carry a coloured mark or a [`Line`] of several colours.
    ///
    /// ```
    /// use tinsel::app::{Frame, Panel};
    /// use tinsel::{Line, palette};
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let title = Line::new().push("● ", palette::RED).push("api", palette::TEXT);
    /// let mut frame = Frame::new(14, 2);
    /// frame.render(frame.area(), Panel::new(title.to_string()));
    /// assert_eq!(frame.snapshot().plain(), "╭─ ● api ────╮\n╰────────────╯");
    /// ```
    pub fn new(title: impl Into<String>) -> Self {
        Panel {
            title: title.into(),
            count: String::new(),
            status: String::new(),
            footer: String::new(),
            body: None,
            focused: false,
            padding: (1, 0),
        }
    }

    /// What goes inside.
    pub fn body(mut self, widget: impl Widget + 'a) -> Self {
        self.body = Some(Box::new(widget));
        self
    }

    /// A count beside the title, in a little pill: how many things are inside, or `3/12`
    /// when some are filtered out.
    ///
    /// ```
    /// use tinsel::app::{Frame, Panel};
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let mut frame = Frame::new(20, 2);
    /// frame.render(frame.area(), Panel::new("services").count(12));
    /// assert_eq!(frame.snapshot().plain(), "╭─ services 12 ────╮\n╰──────────────────╯");
    /// ```
    pub fn count(mut self, count: impl std::fmt::Display) -> Self {
        self.count = count.to_string();
        self
    }

    /// A little text on the top border, at the right: where the panel's data comes from,
    /// when it was read. Styled text keeps its own colours; plain text is muted. When there
    /// isn't room beside the title it's cut short, and left off once fewer than six columns
    /// of it would show.
    ///
    /// ```
    /// use tinsel::app::{Frame, Panel};
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let mut frame = Frame::new(36, 2);
    /// frame.render(frame.area(), Panel::new("services").status("staging · read now"));
    /// assert_eq!(
    ///     frame.snapshot().plain(),
    ///     "╭─ services ── staging · read now ─╮\n╰──────────────────────────────────╯"
    /// );
    /// ```
    pub fn status(mut self, text: impl Into<String>) -> Self {
        self.status = text.into();
        self
    }

    /// A little text on the bottom border, at the right: a count, a hint.
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = text.into();
        self
    }

    /// Whether this is the panel keys go to. Its border shimmers and its title twinkles.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Space between the border and the body: `x` columns each side, `y` rows top and
    /// bottom. `(1, 0)` unless set.
    pub fn padding(mut self, x: usize, y: usize) -> Self {
        self.padding = (x, y);
        self
    }

    /// Where the body goes inside a panel drawn at `area`.
    pub fn inner(&self, area: Rect) -> Rect {
        area.shrink(1, 1).shrink(self.padding.0, self.padding.1)
    }
}

impl Widget for Panel<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.width < 2 || area.height < 2 {
            return;
        }
        frame.clear(area);
        let (w, h, t) = (area.width, area.height, frame.time());
        let perimeter = 2 * (w - 1) + 2 * (h - 1);
        let glint = (t * 18.0).rem_euclid(perimeter as f32);
        let color = |i: usize| {
            if !self.focused {
                return LINE;
            }
            let d = (i as f32 - glint).abs();
            let d = d.min(perimeter as f32 - d);
            mix(ROSE, PETAL, (-(d * d) / 30.0).exp())
        };

        // the title and footer sit on the border, so it's built a cell at a time around them
        let mut title = Line::new().push(" ", LINE);
        if self.focused {
            let b = ((t * 2.6).sin() + 1.0) / 2.0;
            title = title.push("✦ ", sample(&[BERRY, ROSE, SHINE], b * b));
        }
        let pill = if self.count.is_empty() {
            String::new()
        } else if crate::style::color_mode() == crate::style::ColorMode::None {
            format!(" {}", self.count)
        } else {
            let (fg, bg) = if self.focused {
                (PETAL, mix(TRACK, ROSE, 0.35))
            } else {
                (SOFT, mix(TRACK, LINE, 0.4))
            };
            format!(" {}", paint_bg(fg, bg, &format!(" {} ", self.count)))
        };
        let pill_w = text::width(&pill);
        let room = w.saturating_sub(6 + title.width());
        // short of room, the count goes before any of the title does, and with no room at
        // all the title goes too
        let (name, pill) = if text::width(&self.title) + pill_w <= room {
            (self.title.clone(), pill)
        } else if w >= 5 + title.width() {
            (clip(&self.title, room.max(1)), String::new())
        } else {
            (String::new(), String::new())
        };
        // a title with colours of its own is drawn as it comes
        let styled = self.title.contains('\x1b');
        let title = if name.is_empty() {
            Line::new()
        } else if self.focused {
            let name = if styled {
                bold(&name)
            } else {
                bold(&paint(PETAL, &name))
            };
            title.raw(&name).raw(&pill).push(" ", LINE)
        } else if styled {
            title.raw(&name).raw(&pill).push(" ", LINE)
        } else {
            title.push(&name, SOFT).raw(&pill).push(" ", LINE)
        };
        let mut top = Line::new().push("╭", color(0));
        if w > 2 {
            top = top.push("─", color(1)).append(title.clone());
        }
        // the status, with a dash either side of it and a space inside those
        let spare = (w - 1).saturating_sub(top.width() + 4);
        let status = match text::width(&self.status) {
            0 => String::new(),
            n if n <= spare => self.status.clone(),
            _ if spare >= 6 => clip(&self.status, spare),
            _ => String::new(),
        };
        let status_at = (w - 1).saturating_sub(text::width(&status) + 3);
        while top.width() < w - 1 {
            let x = top.width();
            if !status.is_empty() && x == status_at {
                let status = if status.contains('\x1b') {
                    status.clone()
                } else {
                    paint(MUTED, &status)
                };
                top = top.push(" ", LINE).raw(&status).push(" ", LINE);
            } else {
                top = top.push("─", color(x));
            }
        }
        frame.print(area, 0, &top.push("╮", color(w - 1)).to_string());

        for y in 1..h - 1 {
            frame.print(area.row(y), 0, &paint(color(perimeter - y), "│"));
            frame.print(
                Rect::new(area.x + w - 1, area.y + y, 1, 1),
                0,
                &paint(color(w - 1 + y), "│"),
            );
        }

        let footer = if w >= 8 {
            text::truncate(&self.footer, w - 6)
        } else {
            String::new()
        };
        let footer_w = if footer.is_empty() { 0 } else { text::width(&footer) + 2 };
        let bottom_at = |x: usize| (w - 1) + (h - 1) + (w - 1 - x);
        let mut bottom = Line::new().push("╰", color(bottom_at(0)));
        for x in 1..w - 2 - footer_w {
            bottom = bottom.push("─", color(bottom_at(x)));
        }
        if footer_w > 0 {
            bottom = bottom.push(&format!(" {footer} "), MUTED);
        }
        let bottom = bottom
            .push("─", color(bottom_at(w - 2)))
            .push("╯", color(bottom_at(w - 1)));
        frame.print(area.row(h - 1), 0, &bottom.to_string());

        if let Some(body) = &self.body {
            body.render(self.inner(area), frame);
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let (w, h) = match &self.body {
            Some(body) => body.size()?,
            None => (0, 0),
        };
        let count = if self.count.is_empty() {
            0
        } else {
            text::width(&self.count) + 3
        };
        let status = if self.status.is_empty() {
            0
        } else {
            text::width(&self.status) + 3
        };
        let title = text::width(&self.title) + count + status + if self.focused { 9 } else { 7 };
        let (px, py) = self.padding;
        Some(((w + 2 + 2 * px).max(title), h + 2 + 2 * py))
    }
}

// MARK: - List

/// A list to move through, with the chosen row lit up. It scrolls itself to keep that row in
/// view, and shows a scrollbar when there's more than fits. Keep the selected index in your
/// app and move it with [`navigate`](super::navigate).
///
/// ```
/// use tinsel::app::{Frame, List};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(12, 3);
/// frame.render(frame.area(), List::new(["api", "worker", "cron"]).selected(1));
/// assert_eq!(frame.snapshot().plain(), "  api\n› worker\n  cron");
/// ```
pub struct List {
    items: Vec<String>,
    selected: Option<usize>,
}

impl List {
    /// A list of these items. Styled text is fine.
    pub fn new<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        List {
            items: items.into_iter().map(Into::into).collect(),
            selected: None,
        }
    }

    /// Which item is chosen.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }
}

impl Widget for List {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let (n, h) = (self.items.len(), area.height);
        if area.is_empty() {
            return;
        }
        if n == 0 {
            frame.print(area, 0, &paint(MUTED, "nothing here yet"));
            return;
        }
        let chosen = self.selected.map(|s| s.min(n - 1));
        let start = if n <= h {
            0
        } else {
            chosen.unwrap_or(0).saturating_sub(h / 2).min(n - h)
        };
        let overflow = n > h;
        let text_w = area.width.saturating_sub(overflow as usize);
        for (row, i) in (start..n).take(h).enumerate() {
            let line = area.row(row);
            let line = Rect::new(line.x, line.y, text_w, line.height);
            if Some(i) == chosen {
                frame.print(
                    line,
                    0,
                    &format!("{} {}", paint(ROSE, "›"), bold(&paint(TEXT, &self.items[i]))),
                );
                frame.tint(line, mix(TRACK, BERRY, 0.35));
            } else {
                frame.print(line, 0, &format!("  {}", paint(SOFT, &self.items[i])));
            }
        }
        if overflow && area.width > 0 {
            scrollbar(frame, Rect::new(area.x + area.width - 1, area.y, 1, h), start, n);
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let widest = self.items.iter().map(|i| text::width(i)).max().unwrap_or(16);
        Some((widest + 2, self.items.len().max(1)))
    }
}

/// One line in the middle of `area`, for empty and loading states.
pub(super) fn centered(frame: &mut Frame, area: Rect, line: &str) {
    if area.is_empty() {
        return;
    }
    let w = text::width(line).min(area.width);
    let at = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - 1) / 2, w, 1);
    frame.print(at, 0, line);
}

/// A scrollbar down `bar`, for `total` rows of which the ones from `start` are showing.
pub(super) fn scrollbar(frame: &mut Frame, bar: Rect, start: usize, total: usize) {
    let h = bar.height;
    if h == 0 || total <= h {
        return;
    }
    let thumb = (h * h / total).max(1);
    let at = start.min(total - h) * (h - thumb) / (total - h);
    for row in 0..h {
        let mark = if (at..at + thumb).contains(&row) {
            paint(ROSE, "┃")
        } else {
            paint(TRACK, "│")
        };
        frame.print(bar, row, &mark);
    }
}

// MARK: - Tabs

/// A row of tabs, the chosen one a soft pink pill. A tab can have a colour of its own, and
/// a little badge after its title: a count, a mark.
///
/// ```
/// use tinsel::app::{Frame, Tabs};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let tabs = Tabs::new(["overview", "logs"]).selected(1);
/// let mut frame = Frame::new(30, 1);
/// frame.render(frame.area(), &tabs);
/// assert_eq!(frame.snapshot().plain(), " overview   logs");
/// assert_eq!(tabs.at(frame.area(), 13, 0), Some(1));
/// ```
pub struct Tabs {
    tabs: Vec<Tab>,
    selected: usize,
}

struct Tab {
    title: String,
    color: Option<Rgb>,
    badge: Option<(String, Rgb)>,
}

impl Tab {
    /// Columns it takes, padding included.
    fn width(&self) -> usize {
        let badge = self.badge.as_ref().map_or(0, |(b, _)| text::width(b) + 1);
        text::width(&self.title) + badge + 2
    }
}

impl Tabs {
    /// Tabs with these titles.
    pub fn new<S: Into<String>>(titles: impl IntoIterator<Item = S>) -> Self {
        Tabs {
            tabs: titles
                .into_iter()
                .map(|t| Tab {
                    title: t.into(),
                    color: None,
                    badge: None,
                })
                .collect(),
            selected: 0,
        }
    }

    /// Which tab is chosen.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Draw tab `index`'s title in `color`, chosen or not: red for one with errors in it, say.
    /// An index past the last tab is ignored.
    ///
    /// ```
    /// use tinsel::app::Tabs;
    /// use tinsel::palette;
    ///
    /// let tabs = Tabs::new(["overview", "errors"]).color(1, palette::RED);
    /// ```
    pub fn color(mut self, index: usize, color: Rgb) -> Self {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.color = Some(color);
        }
        self
    }

    /// Put a little badge after tab `index`'s title, in `color`: a count, or a mark like
    /// `✔`. Plain text without colour. An index past the last tab is ignored.
    ///
    /// ```
    /// use tinsel::app::{Frame, Tabs};
    /// use tinsel::palette;
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let tabs = Tabs::new(["build", "deploy"]).badge(0, "✔", palette::SAGE).badge(1, "3", palette::RED);
    /// let mut frame = Frame::new(30, 1);
    /// frame.render(frame.area(), &tabs);
    /// assert_eq!(frame.snapshot().plain(), " build ✔   deploy 3");
    /// assert_eq!(tabs.at(frame.area(), 7, 0), Some(0));
    /// ```
    pub fn badge(mut self, index: usize, text: impl Into<String>, color: Rgb) -> Self {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.badge = Some((text.into(), color));
        }
        self
    }

    /// Which tab is at `x`, `y`, if the tabs are drawn at `area`: for clicks.
    pub fn at(&self, area: Rect, x: usize, y: usize) -> Option<usize> {
        if y != area.y || x < area.x {
            return None;
        }
        let mut left = area.x;
        for (i, tab) in self.tabs.iter().enumerate() {
            let right = left + tab.width();
            if (left..right).contains(&x) {
                return Some(i);
            }
            left = right + 1;
        }
        None
    }
}

impl Widget for Tabs {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let line = self.tabs.iter().enumerate().fold(Line::new(), |line, (i, tab)| {
            let line = if i > 0 { line.space(1) } else { line };
            let chosen = i == self.selected;
            let fg = tab.color.unwrap_or(if chosen { PETAL } else { MUTED });
            let mut label = paint(fg, &format!(" {}", tab.title));
            if let Some((badge, color)) = &tab.badge {
                label += &format!(" {}", paint(*color, badge));
            }
            label.push(' ');
            if chosen {
                line.raw(&crate::style::on_bg(mix(TRACK, ROSE, 0.32), &label))
            } else {
                line.raw(&label)
            }
        });
        line.render(area, frame);
    }
}

// MARK: - Plot

/// An area chart of the latest values, as many as fit across, rising berry → blush. Good for
/// anything over time: latency, requests, temperature.
///
/// ```
/// use tinsel::app::{Frame, Plot};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(6, 2);
/// frame.render(frame.area(), Plot::new(&[1.0, 2.0, 3.0, 4.0, 3.0, 2.0]));
/// assert_eq!(frame.snapshot().plain(), "  ▄█▄\n▄█████");
/// ```
pub struct Plot {
    values: Vec<f64>,
    range: Option<(f64, f64)>,
    color: Option<Rgb>,
}

impl Plot {
    /// A chart of these values, oldest first.
    pub fn new(values: &[f64]) -> Self {
        Plot {
            values: values.to_vec(),
            range: None,
            color: None,
        }
    }

    /// The values at the bottom and top of the chart. Unless set, it runs from 0 (or the
    /// lowest value, if that's negative) to the highest.
    pub fn range(mut self, low: f64, high: f64) -> Self {
        self.range = Some((low, high));
        self
    }

    /// Fill in one colour, fading towards the bottom, instead of the pink ramp.
    pub fn color(mut self, color: Rgb) -> Self {
        self.color = Some(color);
        self
    }
}

const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

impl Widget for Plot {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let shown = &self.values[self.values.len().saturating_sub(area.width)..];
        let (lo, hi) = self.range.unwrap_or_else(|| {
            let lo = shown.iter().copied().fold(0.0, f64::min);
            (lo, shown.iter().copied().fold(lo, f64::max))
        });
        let span = (hi - lo).max(f64::EPSILON);
        let h = area.height;
        let left = area.x + area.width - shown.len();
        for row in 0..h {
            // drawn from the bottom up; each row's colour is its height
            let t = (h - 1 - row) as f32 / h.saturating_sub(1).max(1) as f32;
            let color = match self.color {
                Some(c) => mix(mix(TRACK, c, 0.45), c, t),
                None => sample(&RAMP, t),
            };
            let mut line = String::new();
            for &v in shown {
                let eighths = (((v - lo) / span).clamp(0.0, 1.0) * (h * 8) as f64).round() as usize;
                let here = eighths.saturating_sub((h - 1 - row) * 8).min(8);
                line.push_str(if here == 0 { " " } else { LEVELS[here - 1] });
            }
            frame.print(Rect::new(left, area.y, shown.len(), h), row, &paint(color, &line));
        }
    }
}

// MARK: - Stat

/// One number that matters, with a label above it and a note below. With room for it, the
/// number is drawn large.
///
/// ```
/// use tinsel::app::{Frame, Stat};
/// use tinsel::palette;
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(16, 3);
/// frame.render(frame.area(), Stat::new("uptime", "99.9%").note("30 days", palette::SAGE));
/// assert_eq!(frame.snapshot().plain(), "uptime\n99.9%\n30 days");
/// ```
pub struct Stat {
    label: String,
    value: String,
    note: Option<(String, Rgb)>,
}

impl Stat {
    /// A stat called `label`, showing `value`.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Stat {
            label: label.into(),
            value: value.into(),
            note: None,
        }
    }

    /// A line under the number, in a colour: `▲ 12%` in sage, `3 failing` in red.
    pub fn note(mut self, text: impl Into<String>, color: Rgb) -> Self {
        self.note = Some((text.into(), color));
        self
    }
}

impl Widget for Stat {
    fn render(&self, area: Rect, frame: &mut Frame) {
        frame.print(area, 0, &paint(MUTED, &self.label));
        let large = big(&self.value);
        let roomy = area.height > 3 + self.note.is_some() as usize
            && large.lines().iter().all(|l| text::width(l) <= area.width);
        let note_row = if roomy {
            large.render(Rect::new(area.x, area.y + 1, area.width, 3), frame);
            4
        } else {
            frame.print(area, 1, &bold(&gradient(&self.value, &RAMP, 0.0)));
            2
        };
        if let Some((note, color)) = &self.note {
            frame.print(area, note_row, &paint(*color, note));
        }
    }
}

/// Text three rows tall in thin rounded strokes, for clocks and numbers on a dashboard.
/// Digits and `. , : - + % /` are drawn large; anything else sits small on the bottom row.
///
/// ```
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// assert_eq!(tinsel::app::big("12").plain(), "╶┐  ╶─╮\n │  ╭─╯\n╶┴╴ ╰─╴");
/// ```
pub fn big(text: &str) -> Block {
    let mut rows = [String::new(), String::new(), String::new()];
    for (i, c) in text.chars().enumerate() {
        let glyph = match c {
            '0' => ["╭─╮", "│ │", "╰─╯"],
            '1' => ["╶┐ ", " │ ", "╶┴╴"],
            '2' => ["╶─╮", "╭─╯", "╰─╴"],
            '3' => ["╶─╮", " ─┤", "╶─╯"],
            '4' => ["╷ ╷", "╰─┤", "  ╵"],
            '5' => ["╭─╴", "╰─╮", "╶─╯"],
            '6' => ["╭─╴", "├─╮", "╰─╯"],
            '7' => ["╶─┐", "  │", "  ╵"],
            '8' => ["╭─╮", "├─┤", "╰─╯"],
            '9' => ["╭─╮", "╰─┤", "╶─╯"],
            '.' => [" ", " ", "•"],
            ',' => [" ", " ", ","],
            ':' => [" ", "•", "•"],
            '-' => ["  ", "╶╴", "  "],
            '+' => ["   ", "╶┼╴", "   "],
            '%' => ["◦ ╱", " ╱ ", "╱ ◦"],
            '/' => ["  ╱", " ╱ ", "╱  "],
            ' ' => [" ", " ", " "],
            _ => ["", "", ""],
        };
        let small;
        let glyph = if glyph[0].is_empty() && c != ' ' {
            let pad = " ".repeat(text::char_width(c));
            small = c.to_string();
            [pad.clone(), pad, small]
        } else {
            glyph.map(String::from)
        };
        // a little space between glyphs, punctuation included, so a clock's colon sits
        // evenly between its digits
        let gap = i > 0;
        for (row, part) in rows.iter_mut().zip(glyph) {
            if gap {
                row.push(' ');
            }
            row.push_str(&part);
        }
    }
    rows.iter().map(|r| gradient(r, &RAMP, 0.0)).collect()
}

// MARK: - Little touches

/// Key hints for a footer: `q quit · ↑↓ move · tab next panel`.
///
/// ```
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let line = tinsel::app::hints(&[("q", "quit"), ("↑↓", "move")]);
/// assert_eq!(line.to_string(), "q quit · ↑↓ move");
/// ```
pub fn hints(keys: &[(&str, &str)]) -> Line {
    keys.iter().enumerate().fold(Line::new(), |line, (i, (key, what))| {
        let line = if i > 0 { line.push(" · ", TRACK) } else { line };
        line.push(key, BLUSH).space(1).push(what, MUTED)
    })
}

/// The line along the bottom of an app: key hints on the left, in groups, and a status on
/// the right (when it last refreshed, a spinner while something loads).
///
/// When there isn't room for everything, it's fitted like this, so nothing is ever cut off
/// partway through a hint:
///
/// - The status always stays. Only if it's wider than the whole line is it cut short, with
///   "…", and then there are no keys.
/// - The keys share what's left, less a two-column gap before the status. Groups are kept
///   in the order they were added, so put the most important first: whole groups go from
///   the end, and the last group that partly fits keeps as many of its first hints as fit.
///   Nothing after a hint that didn't fit is shown, so later groups never jump ahead.
///
/// ```
/// use tinsel::app::{Footer, Frame};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let footer = Footer::new()
///     .keys(&[("↑↓", "move"), ("/", "filter")])
///     .keys(&[("?", "help"), ("q", "quit")])
///     .status("refreshed 12s ago");
/// let draw = |width| {
///     let mut frame = Frame::new(width, 1);
///     frame.render(frame.area(), &footer);
///     frame.snapshot().plain()
/// };
/// assert_eq!(draw(60), "↑↓ move · / filter   ? help · q quit       refreshed 12s ago");
/// assert_eq!(draw(46), "↑↓ move · / filter   ? help  refreshed 12s ago");
/// assert_eq!(draw(30), "↑↓ move      refreshed 12s ago");
/// assert_eq!(draw(12), "refreshed 1…");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Footer {
    groups: Vec<Vec<(String, String)>>,
    status: String,
}

impl Footer {
    /// An empty footer.
    pub fn new() -> Self {
        Footer::default()
    }

    /// A group of keys and what they do, set a little apart from the others. Groups added
    /// first stay longest when the footer is short of room.
    pub fn keys(mut self, keys: &[(&str, &str)]) -> Self {
        self.groups
            .push(keys.iter().map(|(k, w)| (k.to_string(), w.to_string())).collect());
        self
    }

    /// What's going on, at the right. Styled text is fine: a spinner, a coloured dot.
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    /// The keys that fit in `room` columns, as a line.
    fn fit(&self, room: usize) -> Line {
        let mut line = Line::new();
        for group in &self.groups {
            for (i, (key, what)) in group.iter().enumerate() {
                // three spaces between groups, " · " between hints in one
                let gap = if line.width() == 0 { 0 } else { 3 };
                let hint = text::width(key) + 1 + text::width(what);
                if line.width() + gap + hint > room {
                    return line;
                }
                line = if gap == 0 {
                    line
                } else if i == 0 {
                    line.space(gap)
                } else {
                    line.push(" · ", TRACK)
                };
                line = line.push(key, BLUSH).space(1).push(what, MUTED);
            }
        }
        line
    }
}

impl Widget for Footer {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        let status = clip(&self.status, area.width);
        let status_w = text::width(&status);
        frame.print(
            Rect::new(area.x + area.width - status_w, area.y, status_w, 1),
            0,
            &paint(MUTED, &status),
        );
        let room = area.width.saturating_sub(status_w + 2 * (status_w > 0) as usize);
        frame.print(area, 0, &self.fit(room).to_string());
    }

    fn size(&self) -> Option<(usize, usize)> {
        let keys = self.fit(usize::MAX).width();
        let gap = if keys > 0 && !self.status.is_empty() { 2 } else { 0 };
        Some((keys + gap + text::width(&self.status), 1))
    }
}

/// A scattering of stars twinkling quietly in the background: behind a header, an empty
/// state, a splash screen. Draw it first, then put things on top.
#[derive(Clone, Copy, Debug)]
pub struct Sparkles {
    density: f32,
}

impl Sparkles {
    /// One star for every forty or so cells.
    pub fn new() -> Self {
        Sparkles { density: 0.025 }
    }

    /// The share of cells with a star, 0 to 1.
    pub fn density(mut self, density: f32) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }
}

impl Default for Sparkles {
    fn default() -> Self {
        Sparkles::new()
    }
}

impl Widget for Sparkles {
    fn render(&self, area: Rect, frame: &mut Frame) {
        const STARS: [&str; 4] = ["·", "⋆", "✧", "✦"];
        let t = frame.time();
        for row in 0..area.height {
            for col in 0..area.width {
                let cell = ((area.y + row) * 4093 + area.x + col) as u32;
                if noise(cell, 0) >= self.density {
                    continue;
                }
                // each star keeps its place and twinkles at its own pace
                let speed = 0.6 + 1.8 * noise(cell, 1);
                let b = ((t * speed + noise(cell, 2) * 6.3).sin() + 1.0) / 2.0;
                let b = b * b * b;
                let star = paint(
                    sample(&[TRACK, BERRY, ROSE, SHINE], b),
                    STARS[((b * 4.0) as usize).min(3)],
                );
                frame.print(Rect::new(area.x + col, area.y + row, 1, 1), 0, &star);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_lose_their_margin() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(10, 2);
        f.render(f.area(), Block::from(vec!["  a".to_string(), "    b".to_string()]));
        assert_eq!(f.snapshot().plain(), "a\n  b");
        assert_eq!(drop_columns("\x1b[1m  x", 2), "\x1b[1mx");
    }

    #[test]
    fn cards_fit_the_area_they_are_drawn_in() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(30, 3);
        f.render(Rect::new(0, 0, 14, 3), Card::success("done"));
        assert_eq!(f.snapshot().plain(), "╭────────────╮\n│  ● done    │\n╰────────────╯");
    }

    #[test]
    fn wrapping_carries_styles_and_measures_columns() {
        assert_eq!(wrap("the quick brown fox", 9), ["the quick", "brown fox"]);
        assert_eq!(wrap("a\n\nb", 5), ["a", "", "b"]);
        assert_eq!(
            wrap("\x1b[31maa bb cc\x1b[39m", 2),
            ["\x1b[31maa", "\x1b[31mbb", "\x1b[31mcc\x1b[39m"]
        );
        // a long styled word is broken by columns, not inside its escape code
        assert_eq!(
            wrap("\x1b[38;2;1;2;3mabcdef", 3),
            ["\x1b[38;2;1;2;3mabc", "\x1b[38;2;1;2;3mdef"]
        );
        assert_eq!(wrap("日本語です", 4), ["日本", "語で", "す"]);
        assert_eq!(wrap("\x1b[1mbold\x1b[0m plain", 5), ["\x1b[1mbold\x1b[0m", "plain"]);
    }

    fn drawn(w: usize, h: usize, widget: impl Widget) -> String {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(w, h);
        f.render(f.area(), widget);
        f.snapshot().plain()
    }

    #[test]
    fn stats_go_big_when_there_is_room() {
        let stat = Stat::new("p95", "12").note("ms", crate::palette::SOFT);
        assert_eq!(drawn(10, 5, &stat), "p95\n╶┐  ╶─╮\n │  ╭─╯\n╶┴╴ ╰─╴\nms");
        // too narrow for big digits: plain, with the note right under
        assert_eq!(drawn(4, 5, &stat), "p95\n12\nms\n\n");
        assert_eq!(drawn(10, 4, Stat::new("n", "7")), "n\n╶─┐\n  │\n  ╵");
    }

    #[test]
    fn big_draws_punctuation_and_falls_back_for_letters() {
        crate::set_color_mode(crate::ColorMode::None);
        let rows = |s: &str| big(s).plain();
        assert_eq!(rows("3:4"), "╶─╮   ╷ ╷\n ─┤ • ╰─┤\n╶─╯ •   ╵");
        assert_eq!(rows("-+"), "      \n╶╴ ╶┼╴\n      ");
        assert_eq!(rows("5%"), "╭─╴ ◦ ╱\n╰─╮  ╱ \n╶─╯ ╱ ◦");
        assert_eq!(rows("6/8,9 0"), big("6/8,9 0").plain());
        assert_eq!(rows("1k"), "╶┐   \n │   \n╶┴╴ k");
        assert_eq!(rows("日"), "  \n  \n日");
    }

    #[test]
    fn plots_take_a_range_and_a_colour_and_keep_the_latest_values() {
        let values = [10.0, 0.0, 5.0];
        assert_eq!(drawn(3, 1, Plot::new(&values).range(0.0, 10.0)), "█ ▄");
        assert_eq!(drawn(3, 1, Plot::new(&values).range(5.0, 10.0)), "█");
        assert_eq!(drawn(2, 1, Plot::new(&values).color(crate::palette::SAGE)), " █");
        // fewer values than columns sit at the right
        assert_eq!(drawn(5, 1, Plot::new(&[1.0])), "    █");
        // below zero, the chart starts at the lowest value
        assert_eq!(drawn(3, 1, Plot::new(&[-1.0, 1.0])), "  █");
    }

    #[test]
    fn sparkles_scatter_stars_and_leave_the_rest_alone() {
        let all = drawn(4, 2, Sparkles::new().density(1.0));
        assert_eq!(all.chars().filter(|c| !c.is_whitespace()).count(), 8);
        assert_eq!(drawn(4, 2, Sparkles::default().density(0.0)), "\n");
        assert_eq!(Sparkles::new().density(7.0).density, 1.0);
    }

    #[test]
    fn panels_pad_title_and_focus() {
        let panel = Panel::new("t").body("x").padding(2, 1).focused(true);
        assert_eq!(panel.inner(Rect::new(0, 0, 10, 5)), Rect::new(3, 2, 4, 1));
        assert_eq!(
            drawn(10, 5, panel),
            "╭─ ✦ t ──╮\n│        │\n│  x     │\n│        │\n╰────────╯"
        );
        assert_eq!(drawn(6, 2, Panel::new("").footer("too long")), "╭────╮\n╰────╯");
        assert_eq!(drawn(1, 1, Panel::new("tiny")), "");
    }

    #[test]
    fn counts_give_way_to_titles_and_footers_drop_keys_before_status() {
        assert_eq!(
            drawn(16, 2, Panel::new("services").count(12)).lines().next(),
            Some("╭─ services ───╮")
        );
        assert_eq!(
            drawn(18, 2, Panel::new("services").count(12)).lines().next(),
            Some("╭─ services 12 ──╮")
        );
        assert_eq!(
            drawn(14, 2, Panel::new("services").count(12)).lines().next(),
            Some("╭─ servic… ──╮")
        );
        assert_eq!(Panel::new("ab").count(3).focused(true).size(), Some((15, 2)));
        let footer = Footer::new().keys(&[("q", "quit")]).keys(&[("?", "help")]).status("ok");
        assert_eq!(drawn(16, 1, &footer), "q quit        ok");
        assert_eq!(drawn(20, 1, &footer), "q quit   ? help   ok");
        assert_eq!(footer.size(), Some((19, 1)));
        assert_eq!(drawn(19, 1, &footer), "q quit   ? help  ok");
        assert_eq!(drawn(0, 0, &footer), "");
    }

    #[test]
    fn statuses_sit_top_right_and_give_way_first() {
        let top = |w: usize, panel: Panel| drawn(w, 2, panel).lines().next().unwrap().to_string();
        let panel = || Panel::new("services").count(12).status("staging · read now");
        assert_eq!(top(40, panel()), "╭─ services 12 ─── staging · read now ─╮");
        // short of room the status is cut, then left off, then the count goes, and then the
        // title is cut, until there's no room for it at all
        assert_eq!(top(30, panel()), "╭─ services 12 ─ staging ·… ─╮");
        assert_eq!(top(24, panel()), "╭─ services 12 ────────╮");
        assert_eq!(top(14, panel()), "╭─ servic… ──╮");
        assert_eq!(top(9, panel()), "╭─ s… ──╮");
        assert_eq!(top(8, panel()), "╭─ … ──╮");
        assert_eq!(top(6, panel()), "╭─ … ╮");
        assert_eq!(top(5, panel()), "╭───╮");
        assert_eq!(top(20, Panel::new("").status("live")), "╭─────────── live ─╮");
        // whatever the width, the border stays whole
        for w in 2..48 {
            for panel in [panel(), panel().focused(true)] {
                let line = top(w, panel);
                assert_eq!(text::width(&line), w, "{line}");
                assert!(line.starts_with('╭') && line.ends_with('╮'), "{line}");
            }
        }
        assert_eq!(Panel::new("ab").status("ok").size(), Some((14, 2)));
    }

    #[test]
    fn styled_text_is_clipped_without_breaking_its_codes() {
        assert_eq!(clip("plain text", 6), "plain…");
        assert_eq!(clip("\x1b[31mred\x1b[39m", 5), "\x1b[31mred\x1b[39m");
        assert_eq!(clip("\x1b[31mredder\x1b[39m", 4), "\x1b[31mred…\x1b[0m");
        assert_eq!(text::strip_ansi(&clip("\x1b[1m日本語", 4)), "日…");
    }

    #[test]
    fn footers_drop_whole_hints_and_keep_the_status() {
        let footer = Footer::new()
            .keys(&[("↑↓", "move"), ("enter", "open"), ("/", "filter")])
            .keys(&[("r", "refresh")])
            .status("staging · 3s ago");
        // what's at the left, and that the status is whole at the right
        let keys = |w: usize| {
            let shown = drawn(w, 1, &footer);
            assert!(shown.ends_with("staging · 3s ago"), "{w}: {shown:?}");
            assert!(text::width(&shown) <= w);
            shown[..shown.len() - "staging · 3s ago".len()].trim_end().to_string()
        };
        assert_eq!(keys(61), "↑↓ move · enter open · / filter   r refresh");
        // the second group goes, then the first loses hints from its end
        assert_eq!(keys(60), "↑↓ move · enter open · / filter");
        assert_eq!(keys(49), "↑↓ move · enter open · / filter");
        assert_eq!(keys(48), "↑↓ move · enter open");
        assert_eq!(keys(38), "↑↓ move · enter open");
        assert_eq!(keys(37), "↑↓ move");
        assert_eq!(keys(25), "↑↓ move");
        assert_eq!(keys(24), "");
        assert_eq!(keys(16), "");
        assert_eq!(drawn(10, 1, &footer), "staging ·…");
        // a later group never jumps ahead of one that didn't fit
        let footer = Footer::new()
            .keys(&[("enter", "open the thing")])
            .keys(&[("q", "quit")]);
        assert_eq!(drawn(12, 1, &footer), "");
        for w in 0..80 {
            let shown = drawn(w, 1, Footer::new().keys(&[("a", "bb"), ("c", "dd")]).status("ok"));
            assert!(text::width(&shown) <= w);
            assert!(shown.ends_with("ok") || w < 2, "{w}: {shown:?}");
            let keys = shown.trim_end_matches(['o', 'k', '…']).trim_end();
            assert!(["", "a bb", "a bb · c dd"].contains(&keys), "{w}: {shown:?}");
        }
        assert_eq!(Footer::new().keys(&[("q", "quit")]).size(), Some((6, 1)));
    }

    #[test]
    fn tabs_with_badges_are_wider_and_clicks_follow() {
        let tabs = Tabs::new(["a", "bb", "c"])
            .badge(0, "12", crate::palette::RED)
            .color(2, crate::palette::RED)
            .badge(9, "ignored", crate::palette::RED)
            .color(9, crate::palette::RED)
            .selected(2);
        assert_eq!(drawn(20, 1, &tabs), " a 12   bb   c");
        let area = Rect::new(0, 0, 20, 1);
        let hits: Vec<_> = (0..15).map(|x| tabs.at(area, x, 0)).collect();
        let (a, b, c) = (Some(0), Some(1), Some(2));
        assert_eq!(hits, [a, a, a, a, a, a, None, b, b, b, b, None, c, c, c]);
    }

    #[test]
    fn tabs_find_what_was_clicked() {
        let tabs = Tabs::new(["a", "bb"]);
        let area = Rect::new(2, 1, 20, 1);
        assert_eq!(tabs.at(area, 2, 1), Some(0));
        assert_eq!(tabs.at(area, 5, 1), None); // the gap
        assert_eq!(tabs.at(area, 6, 1), Some(1));
        assert_eq!(tabs.at(area, 10, 1), None);
        assert_eq!(tabs.at(area, 2, 0), None);
        assert_eq!(tabs.at(area, 1, 1), None);
        assert_eq!(drawn(10, 1, tabs), " a   bb");
    }

    #[test]
    fn everything_printable_is_a_widget() {
        assert_eq!(drawn(4, 1, String::from("hi")), "hi");
        assert_eq!(drawn(4, 1, Line::new().push("ok", TEXT)), "ok");
        assert_eq!(drawn(9, 2, Box::new("boxed up") as Box<dyn Widget>), "boxed up\n");
        assert_eq!(
            drawn(12, 2, Table::new(["a", "b"]).row(["1", "2"])).lines().next(),
            Some("a   b")
        );
        assert_eq!(drawn(6, 2, Tree::new("root").leaf("x")).lines().next(), Some("root"));
        assert!(drawn(12, 1, Code::new("let x = 1;")).contains("let x"));
        assert_eq!(drawn(12, 1, List::new(Vec::<String>::new())), "nothing here");
        assert_eq!(drawn(10, 1, hints(&[("q", "quit")])), "q quit");
    }

    #[test]
    fn long_lists_scroll_to_the_selection() {
        crate::set_color_mode(crate::ColorMode::None);
        let items: Vec<String> = (0..20).map(|i| format!("item {i}")).collect();
        let mut f = Frame::new(10, 4);
        f.render(f.area(), List::new(items).selected(19));
        let shown = f.snapshot().plain();
        assert!(shown.contains("› item 19") && !shown.contains("item 15"), "{shown}");
        // and a scrollbar, thumb at the bottom
        assert!(shown.lines().last().unwrap().ends_with('┃'));
    }

    #[test]
    fn lists_in_panels_too_short_for_them_draw_nothing() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(10, 2);
        f.render(f.area(), Panel::new("x").body(List::new(["a", "b"]).selected(1)));
        assert_eq!(f.snapshot().plain(), "╭─ x ────╮\n╰────────╯");
    }

    #[test]
    fn wrapping_keeps_every_kind_of_escape_whole() {
        let osc = "\x1b]8;;https://x.y\x1b\\link\x1b]8;;\x07";
        assert_eq!(wrap(&format!("{osc} b"), 10), [format!("{osc} b")]);
        assert_eq!(wrap("a\x1bc b", 10), ["a\x1bc b"]);
        assert_eq!(wrap("cut\x1b", 10), ["cut\x1b"]);
        // escapes alone take no room, so the leading spaces around them go as usual
        assert_eq!(wrap("\x1b[1m \x1b[0m x", 10), ["\x1b[1m\x1b[0mx"]);
    }

    #[test]
    fn widgets_know_their_size() {
        crate::set_color_mode(crate::ColorMode::None);
        assert_eq!(String::from("ab\ncde").size(), Some((3, 2)));
        assert_eq!(Line::new().push("abcd", TEXT).size(), Some((4, 1)));
        assert_eq!(
            Block::from(vec!["  ab".to_string(), "  abc".to_string()]).size(),
            Some((3, 2))
        );
        assert_eq!(Table::new(["n"]).row(["1"]).size(), Some((1, 3)));
        assert_eq!(Card::success("x").size(), None);
        assert_eq!(Card::success("x").width(20).size().map(|s| s.0), Some(20));
        assert_eq!(List::new(["a", "bcd"]).size(), Some((5, 2)));
        assert_eq!(List::new(Vec::<String>::new()).size(), Some((18, 1)));
    }

    #[test]
    fn dropping_columns_stops_at_text() {
        assert_eq!(drop_columns(" x  y", 3), "x  y");
        assert_eq!(drop_columns("  ", 4), "");
    }

    #[test]
    fn clipping_to_nothing_leaves_no_ellipsis() {
        assert_eq!(clip("\x1b[1mbold\x1b[0m", 0), "\x1b[1m\x1b[0m");
    }

    #[test]
    fn panel_counts_are_pills_in_colour() {
        let drawn = |focused: bool, title: &str| {
            let mut f = Frame::new(30, 3);
            let panel = Panel::new(title).count(7).focused(focused);
            crate::style::with_color_mode(crate::ColorMode::TrueColor, || f.render(f.area(), panel));
            f.snapshot().to_string()
        };
        assert_ne!(drawn(true, "jobs"), drawn(false, "jobs"));
        assert!(drawn(true, "jobs").contains(" 7 "));
        let styled = crate::style::with_color_mode(crate::ColorMode::TrueColor, || paint(crate::palette::SAGE, "ok"));
        assert!(crate::text::strip_ansi(&drawn(true, &styled)).contains("ok"));
    }

    #[test]
    fn nothing_is_centred_or_scrolled_in_no_room() {
        let mut f = Frame::new(4, 2);
        centered(&mut f, Rect::new(0, 0, 0, 0), "x");
        scrollbar(&mut f, Rect::new(0, 0, 1, 0), 0, 10);
        assert_eq!(f.snapshot().plain(), "\n");
        assert_eq!(big("1.5").plain().lines().last().map(|l| l.contains('•')), Some(true));
    }

    #[test]
    fn panels_carry_a_status_and_a_footer() {
        let styled = crate::style::with_color_mode(crate::ColorMode::TrueColor, || paint(crate::palette::SAGE, "jobs"));
        let running =
            crate::style::with_color_mode(crate::ColorMode::TrueColor, || paint(crate::palette::ROSE, "2 running"));
        let panel = Panel::new(styled).status(running).footer("q quit").focused(false);
        let mut f = Frame::new(40, 4);
        crate::style::with_color_mode(crate::ColorMode::TrueColor, || f.render(f.area(), panel));
        let shown = f.snapshot().plain();
        assert!(
            shown.contains("jobs") && shown.contains("2 running") && shown.contains("q quit"),
            "{shown}"
        );
    }

    #[test]
    fn a_short_list_needs_no_scrollbar() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(10, 4);
        f.render(f.area(), List::new(["a", "b"]).selected(1));
        assert_eq!(f.snapshot().plain(), "  a\n› b\n\n");
    }
}

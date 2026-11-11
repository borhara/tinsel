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
fn wrap(s: &str, cols: usize) -> Vec<String> {
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
    footer: String,
    body: Option<Box<dyn Widget + 'a>>,
    focused: bool,
    padding: (usize, usize),
}

impl<'a> Panel<'a> {
    /// A panel with this title (which can be empty).
    pub fn new(title: impl Into<String>) -> Self {
        Panel {
            title: title.into(),
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
        let name = text::truncate(&self.title, w.saturating_sub(6 + title.width()));
        let title = if self.title.is_empty() {
            Line::new()
        } else if self.focused {
            title.raw(&bold(&paint(PETAL, &name))).push(" ", LINE)
        } else {
            title.push(&name, SOFT).push(" ", LINE)
        };
        let mut top = Line::new()
            .push("╭", color(0))
            .push("─", color(1))
            .append(title.clone());
        for x in top.width()..w - 1 {
            top = top.push("─", color(x));
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
        let title = text::width(&self.title) + if self.focused { 9 } else { 7 };
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

/// A row of tabs, the chosen one a soft pink pill.
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
    titles: Vec<String>,
    selected: usize,
}

impl Tabs {
    /// Tabs with these titles.
    pub fn new<S: Into<String>>(titles: impl IntoIterator<Item = S>) -> Self {
        Tabs {
            titles: titles.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }

    /// Which tab is chosen.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Which tab is at `x`, `y`, if the tabs are drawn at `area`: for clicks.
    pub fn at(&self, area: Rect, x: usize, y: usize) -> Option<usize> {
        if y != area.y || x < area.x {
            return None;
        }
        let mut left = area.x;
        for (i, title) in self.titles.iter().enumerate() {
            let right = left + text::width(title) + 2;
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
        let line = self.titles.iter().enumerate().fold(Line::new(), |line, (i, title)| {
            let line = if i > 0 { line.space(1) } else { line };
            if i == self.selected {
                line.raw(&paint_bg(PETAL, mix(TRACK, ROSE, 0.32), &format!(" {title} ")))
            } else {
                line.push(&format!(" {title} "), MUTED)
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
}

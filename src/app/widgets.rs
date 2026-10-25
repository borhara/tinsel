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
}

impl<W: Widget + ?Sized> Widget for &W {
    fn render(&self, area: Rect, frame: &mut Frame) {
        (**self).render(area, frame)
    }
}

impl<W: Widget + ?Sized> Widget for Box<W> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        (**self).render(area, frame)
    }
}

/// Text is word-wrapped to fit. Styled text is fine.
impl Widget for str {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let lines = self.lines().flat_map(|l| text::wrap(l, area.width.max(1)));
        for (row, line) in lines.take(area.height).enumerate() {
            frame.print(area, row, &line);
        }
    }
}

impl Widget for String {
    fn render(&self, area: Rect, frame: &mut Frame) {
        self.as_str().render(area, frame)
    }
}

impl Widget for Line {
    fn render(&self, area: Rect, frame: &mut Frame) {
        frame.print(area, 0, &self.to_string());
    }
}

/// Blocks are drawn a line per row, cut off rather than wrapped. The two-space margin tinsel
/// gives printed output is taken off, since the block is placed exactly where it's wanted.
impl Widget for Block {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let margin = self
            .lines()
            .iter()
            .map(|l| text::strip_ansi(l))
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);
        for (row, line) in self.lines().iter().take(area.height).enumerate() {
            frame.print(area, row, &drop_columns(line, margin));
        }
    }
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
        }
    )*};
}

via_lines!(Table, Tree, Card, Code);

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
            let thumb = (h * h / n).max(1);
            let at = start * (h - thumb) / (n - h);
            let bar = Rect::new(area.x + area.width - 1, area.y, 1, h);
            for row in 0..h {
                let mark = if (at..at + thumb).contains(&row) {
                    paint(ROSE, "┃")
                } else {
                    paint(TRACK, "│")
                };
                frame.print(bar, row, &mark);
            }
        }
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

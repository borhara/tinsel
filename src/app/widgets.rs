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

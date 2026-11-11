//! Things drawn over the rest of an app: modals and toasts.

use super::widgets::wrap;
use super::{Frame, Rect, Widget};
use crate::Tone;
use crate::style::palette::{AMBER, BLUSH, LINE, MUTED, RED, ROSE, TEXT, TRACK};
use crate::style::{Rgb, mix, paint};

/// How long a modal takes to settle into place, in seconds.
const ENTRANCE: f32 = 0.18;

/// Something drawn in the middle of the screen over everything else, which fades back
/// behind it: a dialog, a picker, the help. Usually a focused [`Panel`](super::Panel). Draw
/// it last, over the whole screen.
///
/// It's as big as what's inside wants to be (see [`Widget::size`]), or as
/// [`size`](Modal::size) says, and never bigger than the screen.
///
/// ```
/// use tinsel::app::{Frame, Modal, Panel};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(20, 5);
/// frame.print(frame.area(), 2, "behind it all");
/// frame.render(frame.area(), Modal::new(Panel::new("sure?").body("y / n")));
/// assert_eq!(
///     frame.snapshot().plain(),
///     "\n    ╭─ sure? ──╮\nbehi│ y / n    │\n    ╰──────────╯\n"
/// );
/// ```
pub struct Modal<'a> {
    body: Box<dyn Widget + 'a>,
    size: Option<(usize, usize)>,
    opened: Option<f32>,
}

impl<'a> Modal<'a> {
    /// A modal holding this widget.
    pub fn new(body: impl Widget + 'a) -> Self {
        Modal {
            body: Box::new(body),
            size: None,
            opened: None,
        }
    }

    /// Make it `width` by `height`, whatever's inside.
    pub fn size(mut self, width: usize, height: usize) -> Self {
        self.size = Some((width, height));
        self
    }

    /// When it was opened, from [`app::time`](super::time) (or [`Frame::time`]), so it can
    /// rise gently into place instead of appearing all at once.
    pub fn opened(mut self, seconds: f32) -> Self {
        self.opened = Some(seconds);
        self
    }

    /// Where it goes when drawn over `screen`, once it's settled: for clicks.
    pub fn area(&self, screen: Rect) -> Rect {
        let (w, h) = self
            .size
            .or_else(|| self.body.size())
            .unwrap_or((screen.width / 2, screen.height / 2));
        // a little of what's behind stays showing around it, where there's room
        let widest = if screen.width > 8 {
            screen.width - 4
        } else {
            screen.width
        };
        screen.center(w.min(widest), h)
    }
}

impl Widget for Modal<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let mut at = self.area(area);
        frame.dim(area);
        if let Some(opened) = self.opened {
            let p = ((frame.time() - opened) / ENTRANCE).clamp(0.0, 1.0);
            let rise = (1.0 - p).powi(3) * 2.0;
            at.y = (at.y + rise.round() as usize).min((area.y + area.height).saturating_sub(at.height));
        }
        // a soft shadow below and to the right
        let shadow = Rgb(14, 10, 12);
        frame.tint(
            Rect::new(at.x + 2, at.y + at.height, at.width, 1).intersect(area),
            shadow,
        );
        frame.tint(
            Rect::new(at.x + at.width, at.y + 1, 2, at.height).intersect(area),
            shadow,
        );
        frame.clear(at);
        self.body.render(at, frame);
    }
}

// MARK: - Toasts

/// How long a toast takes to slide in, and out again.
const SLIDE: f32 = 0.25;

/// Short messages that slide into a corner, stay a few seconds and go: "saved", "couldn't
/// connect". Keep one in your app, [`push`](Toasts::push) to it from
/// [`event`](super::App::event), and draw it last, over the area the toasts should sit in
/// the bottom right of. A line along each one's foot runs down until it goes.
///
/// ```
/// use tinsel::Tone;
/// use tinsel::app::{Frame, Toasts};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut toasts = Toasts::new();
/// toasts.push(Tone::Success, "deployed my-app");
/// let mut frame = Frame::new(30, 3).at(1.0);
/// frame.render(frame.area(), &toasts);
/// assert_eq!(
///     frame.snapshot().plain(),
///     "       ╭─────────────────────╮\n       │ ●  deployed my-app  │\n       ╰━━━━━━━━━━━━━━━━─────╯"
/// );
/// ```
#[derive(Clone, Debug)]
pub struct Toasts {
    shown: Vec<Toast>,
    life: f32,
}

#[derive(Clone, Debug)]
struct Toast {
    tone: Tone,
    text: String,
    at: f32,
}

impl Default for Toasts {
    fn default() -> Self {
        Toasts::new()
    }
}

impl Toasts {
    /// No toasts yet. Each stays for four seconds unless [`life`](Toasts::life) says.
    pub fn new() -> Self {
        Toasts {
            shown: Vec::new(),
            life: 4.0,
        }
    }

    /// How many seconds each toast stays.
    pub fn life(mut self, seconds: f32) -> Self {
        self.life = seconds.max(SLIDE * 2.0);
        self
    }

    /// Show a message, marked and coloured by its tone, from now (by [`app::time`](super::time)).
    pub fn push(&mut self, tone: Tone, text: impl Into<String>) {
        let now = super::time();
        self.shown.retain(|t| now - t.at < self.life);
        self.shown.push(Toast {
            tone,
            text: text.into(),
            at: now,
        });
    }

    /// Whether any toast is still showing at `seconds`.
    pub fn showing(&self, seconds: f32) -> bool {
        self.shown.iter().any(|t| (t.at..t.at + self.life).contains(&seconds))
    }
}

fn tone_look(tone: Tone) -> (&'static str, Rgb) {
    match tone {
        Tone::Success => ("●", ROSE),
        Tone::Info => ("›", BLUSH),
        Tone::Warn => ("▲", AMBER),
        Tone::Error => ("✕", RED),
        Tone::Hint => ("→", MUTED),
    }
}

impl Widget for Toasts {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let now = frame.time();
        let widest = (area.width * 2 / 5).max(32).min(area.width);
        let mut bottom = area.y + area.height;
        // newest at the bottom, older ones stacked above until there's no more room
        for toast in self.shown.iter().rev() {
            let age = now - toast.at;
            if !(0.0..self.life).contains(&age) {
                continue;
            }
            let (mark, color) = tone_look(toast.tone);
            let inner = widest.saturating_sub(8).max(1);
            let lines = wrap(&toast.text, inner);
            let text_w = lines.iter().map(|l| crate::text::width(l)).max().unwrap_or(0);
            let w = (text_w + 8).min(area.width);
            let h = lines.len() + 2;
            if w < 8 || bottom < area.y + h {
                break;
            }
            let y = bottom - h;
            bottom = y;

            // slides in from the right, and back out at the end
            let ease = |p: f32| 1.0 - (1.0 - p.clamp(0.0, 1.0)).powi(3);
            let shown = ease(age / SLIDE).min(ease((self.life - age) / SLIDE));
            let hidden = ((1.0 - shown) * (w + 1) as f32).round() as usize;
            let x = area.x + area.width - w + hidden.min(w);
            let visible = Rect::new(x, y, area.x + area.width - x, h);
            if visible.width == 0 {
                continue;
            }

            let edge = mix(LINE, color, 0.55);
            let left = 1.0 - age / self.life;
            let run = ((w - 2) as f32 * left).round() as usize;
            let rows = std::iter::once(paint(edge, &format!("╭{}╮", "─".repeat(w - 2))))
                .chain(lines.iter().enumerate().map(|(i, line)| {
                    let lead = if i == 0 { paint(color, mark) } else { " ".into() };
                    let pad = " ".repeat(w.saturating_sub(8 + crate::text::width(line)));
                    format!(
                        "{} {lead}  {}{pad}  {}",
                        paint(edge, "│"),
                        paint(TEXT, line),
                        paint(edge, "│")
                    )
                }))
                .chain(std::iter::once(format!(
                    "{}{}{}{}",
                    paint(edge, "╰"),
                    paint(color, &"━".repeat(run)),
                    paint(TRACK, &"─".repeat(w - 2 - run)),
                    paint(edge, "╯")
                )));
            frame.clear(visible);
            frame.tint(visible, mix(Rgb(20, 16, 18), color, 0.08));
            for (row, line) in rows.enumerate() {
                frame.print(visible, row, &line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Panel;

    #[test]
    fn modals_size_themselves_and_fit_the_screen() {
        let screen = Rect::new(0, 0, 40, 10);
        assert_eq!(Modal::new("hello").area(screen), Rect::new(17, 4, 5, 1));
        assert_eq!(Modal::new("x").size(100, 3).area(screen), Rect::new(2, 3, 36, 3));
        // nothing to go on: half the screen
        assert_eq!(
            Modal::new(Panel::new("t").body(Unsized)).area(screen),
            Rect::new(10, 2, 20, 5)
        );
    }

    struct Unsized;

    impl Widget for Unsized {
        fn render(&self, _: Rect, _: &mut Frame) {}
    }

    #[test]
    fn toasts_slide_in_stack_and_go() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut toasts = Toasts::default().life(3.0);
        toasts.push(Tone::Error, "couldn't connect to the database server");
        toasts.push(Tone::Info, "retrying");
        let drawn = |t: f32| {
            let mut f = Frame::new(40, 8).at(t);
            f.render(f.area(), &toasts);
            f.snapshot().plain()
        };
        // not in yet, then all the way in, newest at the bottom and long ones wrapped
        assert_eq!(drawn(0.0).trim(), "");
        let settled = drawn(1.0);
        assert!(
            settled
                .lines()
                .nth(2)
                .unwrap()
                .ends_with("│ ✕  couldn't connect to the  │"),
            "{settled}"
        );
        assert!(settled.lines().nth(6).unwrap().contains("›  retrying"), "{settled}");
        // partway in, cut off at the right edge
        assert!(drawn(0.1).lines().last().unwrap().trim_start().starts_with('╰'));
        assert!(!drawn(0.1).contains('╯'));
        assert!(toasts.showing(2.9) && !toasts.showing(3.0));
        assert_eq!(drawn(3.5).trim(), "");
        // too short a screen for both: the newest wins
        let mut f = Frame::new(40, 4).at(1.0);
        f.render(f.area(), &toasts);
        assert!(f.snapshot().plain().contains("retrying") && !f.snapshot().plain().contains("connect"));
    }

    #[test]
    fn modals_rise_into_place() {
        crate::set_color_mode(crate::ColorMode::None);
        let at = |t: f32| {
            let mut f = Frame::new(12, 7).at(t);
            f.render(f.area(), Modal::new(Panel::new("").body("hi")).opened(1.0));
            f.snapshot().plain().lines().position(|l| l.contains('╭'))
        };
        assert_eq!(at(1.0), Some(4));
        assert_eq!(at(1.05), Some(3));
        assert_eq!(at(2.0), Some(2));
    }
}

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
    at: Option<Rect>,
    opened: Option<f64>,
    background: Option<Rgb>,
    dim: bool,
}

impl std::fmt::Debug for Modal<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modal")
            .field("size", &self.size)
            .field("at", &self.at)
            .field("opened", &self.opened)
            .field("background", &self.background)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

impl<'a> Modal<'a> {
    /// A modal holding this widget.
    pub fn new(body: impl Widget + 'a) -> Self {
        Modal {
            body: Box::new(body),
            size: None,
            at: None,
            opened: None,
            background: None,
            dim: true,
        }
    }

    /// Make it `width` by `height`, whatever's inside.
    #[must_use]
    pub fn size(mut self, width: usize, height: usize) -> Self {
        self.size = Some((width, height));
        self
    }

    /// Put it exactly here, rather than in the middle of the screen: high up, say, so what
    /// it's about shows below it. Kept on the screen.
    ///
    /// ```
    /// use tinsel::app::{Modal, Rect};
    ///
    /// let screen = Rect::new(0, 0, 40, 20);
    /// let placed = Modal::new("hi").at(Rect::new(5, 2, 30, 6));
    /// assert_eq!(placed.area(screen), Rect::new(5, 2, 30, 6));
    /// ```
    #[must_use]
    pub fn at(mut self, rect: Rect) -> Self {
        self.at = Some(rect);
        self
    }

    /// When it was opened, from [`app::time`](super::time) (or [`Frame::time`]), so it can
    /// rise gently into place, with what's behind it fading as it comes, instead of
    /// appearing all at once. It asks for the frames it needs to.
    #[must_use]
    pub fn opened(mut self, seconds: f64) -> Self {
        self.opened = Some(seconds);
        self
    }

    /// Fill it in this colour, as [`Panel::background`](super::Panel::background) does a
    /// panel, so it stands apart from what's behind.
    #[must_use]
    pub fn background(mut self, color: Rgb) -> Self {
        self.background = Some(color);
        self
    }

    /// Whether what's behind fades back. Yes unless changed; a small popup that shouldn't
    /// take over the screen says no.
    #[must_use]
    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }

    /// How far it has come into place at `t` seconds, from 0 as it opens to 1 once settled:
    /// 1 throughout unless it was [`opened`](Modal::opened).
    pub fn shown(&self, t: f64) -> f32 {
        let Some(opened) = self.opened else { return 1.0 };
        let p = ((t - opened) as f32 / ENTRANCE).clamp(0.0, 1.0);
        1.0 - (1.0 - p).powi(3)
    }

    /// Where it is when drawn over `screen` at `t` seconds, rising into place.
    fn placed(&self, screen: Rect, t: f64) -> Rect {
        let mut at = self.area(screen);
        let rise = (1.0 - self.shown(t)) * 2.0;
        at.y = (at.y + rise.round() as usize).min((screen.y + screen.height).saturating_sub(at.height));
        at
    }

    /// Where it goes when drawn over `screen`, once it's settled: for clicks.
    pub fn area(&self, screen: Rect) -> Rect {
        if let Some(at) = self.at {
            return at.intersect(screen);
        }
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
        let shown = self.shown(frame.time());
        if shown < 1.0 {
            frame.animate();
        }
        let at = self.placed(area, frame.time());
        if self.dim {
            frame.fade_by(area, shown);
        }
        frame.shadow_by(at, shown);
        frame.clear(at);
        if let Some(bg) = self.background {
            frame.tint(at, bg);
        }
        self.body.render(at, frame);
    }

    fn inner(&self, area: Rect) -> Rect {
        self.body.inner(area)
    }
}

impl Modal<'_> {
    /// Where what's inside goes when drawn over `screen` at `t` seconds, for
    /// [`Frame::modal`].
    pub(super) fn inner_at(&self, screen: Rect, t: f64) -> Rect {
        self.body.inner(self.placed(screen, t))
    }
}

// MARK: - Toasts

/// How long a toast takes to slide in, and out again.
const SLIDE: f32 = 0.25;
/// How often the line along a toast's foot moves on, in frames a second.
const COUNTDOWN_FPS: u32 = 30;

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
/// toasts.push(0.0, Tone::Success, "deployed my-app");
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
    life: f64,
}

#[derive(Clone, Debug)]
struct Toast {
    tone: Tone,
    text: String,
    at: f64,
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
    #[must_use]
    pub fn life(mut self, seconds: f64) -> Self {
        self.life = seconds.max(SLIDE as f64 * 2.0);
        self
    }

    /// Show a message, marked and coloured by its tone, from `now`: [`app::time`](super::time)
    /// in [`App::event`](super::App::event), as for [`Modal::opened`].
    pub fn push(&mut self, now: f64, tone: Tone, text: impl Into<String>) {
        self.shown.retain(|t| now - t.at < self.life);
        self.shown.push(Toast {
            tone,
            text: text.into(),
            at: now,
        });
    }

    /// Whether any toast is still showing at `seconds`.
    pub fn showing(&self, seconds: f64) -> bool {
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
            if now < toast.at {
                // pushed for later: it comes in then
                frame.redraw_at(toast.at);
                continue;
            }
            if now - toast.at >= self.life {
                continue;
            }
            // sliding in or out is motion; in between, only the line along its foot moves,
            // which can rest while nobody is looking, but it still goes on time
            let leaving = toast.at + self.life - f64::from(SLIDE);
            if now - toast.at < f64::from(SLIDE) || now >= leaving {
                frame.animate();
            } else {
                frame.animate_at(COUNTDOWN_FPS);
                frame.redraw_at(leaving);
            }
            // small once it's known to be within the toast's life
            let (age, life) = ((now - toast.at) as f32, self.life as f32);
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
            let shown = ease(age / SLIDE).min(ease((life - age) / SLIDE));
            let hidden = ((1.0 - shown) * (w + 1) as f32).round() as usize;
            let x = area.x + area.width - w + hidden.min(w);
            let visible = Rect::new(x, y, area.x + area.width - x, h);
            if visible.width == 0 {
                continue;
            }

            let edge = mix(LINE, color, 0.55);
            let left = 1.0 - age / life;
            let span = (w - 2) as f32 * left.clamp(0.0, 1.0);
            let run = span.floor() as usize;
            // the cell where it ends fades between the two, so the line shrinks smoothly rather
            // than a whole cell at a time
            let part = span - run as f32;
            let edge_cell = if run < w - 2 {
                paint(mix(TRACK, color, part), if part >= 0.5 { "━" } else { "─" })
            } else {
                String::new()
            };
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
                    "{}{}{edge_cell}{}{}",
                    paint(edge, "╰"),
                    paint(color, &"━".repeat(run)),
                    paint(TRACK, &"─".repeat((w - 2).saturating_sub(run + 1))),
                    paint(edge, "╯")
                )));
            // no fill: the rows overwrite what's behind, and a filled border would show as a square
            // frame around the round corners, since the line runs through the middle of its cells
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
        toasts.push(0.0, Tone::Error, "couldn't connect to the database server");
        toasts.push(0.0, Tone::Info, "retrying");
        let drawn = |t: f64| {
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
        let at = |t: f64| {
            let mut f = Frame::new(12, 7).at(t);
            f.render(f.area(), Modal::new(Panel::new("").body("hi")).opened(1.0));
            f.snapshot().plain().lines().position(|l| l.contains('╭'))
        };
        assert_eq!(at(1.0), Some(4));
        assert_eq!(at(1.05), Some(3));
        assert_eq!(at(2.0), Some(2));
    }

    #[test]
    fn the_shadow_keeps_what_is_under_it() {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(20, 7);
        for row in 0..7 {
            f.print(f.area(), row, "api ok web ok db ok!");
        }
        f.render(f.area(), Modal::new(Panel::new("sure?").body("y / n")));
        let shown = f.snapshot().plain();
        let rows: Vec<&str> = shown.lines().collect();
        // the rows below and beside the dialog read just as they did
        assert_eq!(rows[5], "api ok web ok db ok!", "{shown}");
        assert!(rows[3].ends_with("│ ok!"), "{shown}");
    }

    #[test]
    fn toasts_and_modals_ask_for_the_frames_they_need() {
        let mut toasts = Toasts::new().life(2.0);
        toasts.push(1.0, Tone::Info, "hello");
        let asks = |t: f64, w: &dyn Widget| {
            let mut f = Frame::new(40, 6).at(t);
            w.render(f.area(), &mut f);
            (f.wants().motion, f.wants().ambient.is_some(), f.wants().at)
        };
        assert_eq!(asks(0.5, &toasts), (false, false, Some(1.0)), "comes in when pushed");
        assert_eq!(asks(1.1, &toasts), (true, false, None), "sliding in");
        assert_eq!(asks(2.0, &toasts), (false, true, Some(2.75)), "counting down");
        assert_eq!(asks(2.9, &toasts), (true, false, None), "sliding out");
        assert_eq!(asks(3.1, &toasts), (false, false, None), "gone");

        let modal = Modal::new("hi").opened(1.0);
        assert!(asks(1.1, &modal).0);
        assert_eq!(asks(2.0, &modal), (false, false, None));
    }

    #[test]
    fn a_modal_can_be_placed_filled_and_left_undimmed() {
        crate::with_color_mode(crate::ColorMode::TrueColor, || {
            let drawn = |modal: &Modal, t: f64| {
                let mut f = Frame::new(20, 8).at(t);
                f.print(f.area(), 7, "behind");
                let inner = f.modal(modal);
                (inner, f.html_pre())
            };
            let plain = |m: Modal| {
                let mut f = Frame::new(20, 8);
                f.print(f.area(), 7, "behind");
                f.render(f.area(), m);
                f.html_pre()
            };
            let placed = Modal::new(Panel::new("x")).at(Rect::new(1, 1, 10, 4));
            let (inner, _) = drawn(&placed, 0.0);
            assert_eq!(inner, Rect::new(3, 2, 6, 2));
            // left as it was behind, or faded back
            let kept = plain(Modal::new("hi").at(Rect::new(0, 0, 4, 1)).dim(false));
            let faded = plain(Modal::new("hi").at(Rect::new(0, 0, 4, 1)));
            assert_ne!(kept, faded);
            let filled = plain(Modal::new("hi").at(Rect::new(0, 0, 4, 1)).background(Rgb(9, 8, 7)));
            assert!(filled.contains("background:#090807"), "{filled}");
            // rising in, it's lower down and what's behind is only partly faded
            let opening = Modal::new(Panel::new("x")).at(Rect::new(1, 1, 10, 4)).opened(1.0);
            let (rising, partly) = drawn(&opening, 1.02);
            let (settled, fully) = drawn(&opening, 2.0);
            assert!(rising.y > settled.y);
            assert_ne!(partly, fully);
            assert!(opening.shown(1.02) < 1.0 && opening.shown(2.0) == 1.0);
        });
    }

    #[test]
    fn a_narrow_screen_isnt_given_a_margin() {
        let screen = Rect::new(0, 0, 6, 4);
        assert_eq!(Modal::new("a long message").area(screen).width, 6);
    }

    #[test]
    fn every_tone_has_its_own_mark() {
        let marks: Vec<&str> = [Tone::Success, Tone::Info, Tone::Warn, Tone::Error, Tone::Hint]
            .into_iter()
            .map(|t| tone_look(t).0)
            .collect();
        assert_eq!(marks, ["●", "›", "▲", "✕", "→"]);
        let mut f = Frame::new(4, 1);
        Unsized.render(f.area(), &mut f);
    }
}

//! Things drawn over the rest of an app: modals.

use super::{Frame, Rect, Widget};
use crate::style::Rgb;

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

//! The pieces tinsel animates with, for drawing your own: a light passing over a label, the
//! twinkle a finished step shows, and bars still on the move. Each takes the seconds since
//! the animation started, so it can be drawn at any moment, as often as you like.
//!
//! For values that move, a [`Glide`] eases from one to another, and a [`Glider`] glides
//! to wherever it's told to be next, asking an [app](crate::app)'s frames for more until it
//! lands.
//!
//! ```
//! use tinsel::anim;
//!
//! let t = 1.5;
//! println!("{} uploading {}", tinsel::Spinner::Sparkle.frame(t), anim::bar(0.4, 20, t));
//! println!("{}", anim::shimmer("thinking", t));
//! ```

pub use crate::widgets::{live_bar as bar, live_waveform as waveform, pop, shimmer};

use std::cell::Cell;

use crate::app::Frame;

/// How far back a new glide starts, so the first frame drawn has already moved: one frame at
/// 60 a second.
const LEAD: f64 = 1.0 / 60.0;

/// An eased move from one value to another, starting quickly and settling gently into
/// place: a scroll position, a column sliding over, a marker moving along a bar. Ask it where
/// the value is at any moment with [`at`](Glide::at).
///
/// ```
/// use tinsel::anim::Glide;
///
/// let glide = Glide::new(0.0, 10.0, 1.0, 0.5);
/// assert_eq!(glide.at(0.0), 0.0);
/// assert!(glide.at(1.25) > 5.0, "more than halfway by half time");
/// assert_eq!(glide.at(1.5), 10.0);
/// assert_eq!(glide.end(), 1.5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    from: f64,
    to: f64,
    start: f64,
    length: f64,
}

impl Glide {
    /// A glide from `from` to `to`, starting at `start` seconds and taking `seconds`.
    pub fn new(from: f64, to: f64, start: f64, seconds: f64) -> Self {
        Glide {
            from,
            to,
            start,
            length: seconds.max(0.0),
        }
    }

    /// Where the value is at `t` seconds: `from` before it starts, `to` once it's over.
    pub fn at(&self, t: f64) -> f64 {
        if self.length <= 0.0 {
            return if t < self.start { self.from } else { self.to };
        }
        let p = ((t - self.start) / self.length).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - p).powi(3);
        self.from + (self.to - self.from) * eased
    }

    /// When it's over, in seconds.
    pub fn end(&self) -> f64 {
        self.start + self.length
    }

    /// Whether it's still on its way at `t` seconds.
    pub fn moving(&self, t: f64) -> bool {
        t < self.end()
    }

    /// Where it started from.
    pub fn from(&self) -> f64 {
        self.from
    }

    /// Where it's going.
    pub fn to(&self) -> f64 {
        self.to
    }

    /// The same glide, on its way somewhere else instead: for a target that moves while
    /// it's gliding, like a column sliding to wherever the text now starts.
    ///
    /// ```
    /// use tinsel::anim::Glide;
    ///
    /// let glide = Glide::new(0.0, 10.0, 0.0, 1.0).towards(20.0);
    /// assert_eq!(glide.at(1.0), 20.0);
    /// ```
    #[must_use]
    pub fn towards(self, to: f64) -> Self {
        Glide { to, ..self }
    }
}

/// A value that glides to wherever it's told to be, rather than jumping: a list scrolling to
/// its new top, a marker following a cursor. Tell it where the value should be each frame
/// with [`follow`](Glider::follow); when that changes, it glides there from wherever it was
/// showing, even partway through another glide, and asks for frames until it lands.
///
/// It keeps what it last saw inside, so it can be kept in an app and followed from
/// [`App::view`](crate::app::App::view), which only borrows the app. A frame at time 0, as in
/// [`snapshot`](crate::app::snapshot) and most tests, always shows where it lands.
///
/// ```
/// use tinsel::anim::Glider;
/// use tinsel::app::Frame;
///
/// let top = Glider::new().snap(2.0);
/// let mut frame = Frame::new(10, 10).at(1.0);
/// assert_eq!(top.follow(0.0, 10.0, &mut frame), 0.0);
///
/// // the list scrolls down a page: on its way there, and asking for frames until it lands
/// let mut frame = Frame::new(10, 10).at(1.05);
/// let shown = top.follow(10.0, 10.0, &mut frame);
/// assert!(shown > 0.0 && shown < 10.0 && frame.wants_redraw());
///
/// let mut frame = Frame::new(10, 10).at(2.0);
/// assert_eq!(top.follow(10.0, 10.0, &mut frame), 10.0);
/// assert!(!frame.wants_redraw());
/// ```
#[derive(Clone, Debug)]
pub struct Glider {
    /// The value and span it was last told.
    seen: Cell<Option<(f64, f64)>>,
    glide: Cell<Option<Glide>>,
    snap: f64,
    short: f64,
    long: f64,
}

impl Default for Glider {
    fn default() -> Self {
        Glider::new()
    }
}

impl Glider {
    /// A glider with nothing seen yet, which glides any move, in 0.12 seconds for a short one
    /// up to 0.18 for one as far as the span.
    pub fn new() -> Self {
        Glider {
            seen: Cell::new(None),
            glide: Cell::new(None),
            snap: 0.0,
            short: 0.12,
            long: 0.18,
        }
    }

    /// Jump straight to a new value no further away than `distance`, rather than gliding:
    /// so holding a key down to scroll a line at a time never lags behind.
    #[must_use]
    pub fn snap(mut self, distance: f64) -> Self {
        self.snap = distance.max(0.0);
        self
    }

    /// How long a glide takes: `short` seconds for the smallest move, up to `long` for one
    /// as far as the span or further.
    #[must_use]
    pub fn duration(mut self, short: f64, long: f64) -> Self {
        self.short = short.max(0.0);
        self.long = long.max(self.short);
        self
    }

    /// Where the value shows at `t` seconds, now that it should be `to`. `span` is how far
    /// counts as a long way, such as the rows a page shows: a glide over the whole span takes
    /// the longest, and when the span itself changes (the screen was resized) the value
    /// jumps instead.
    ///
    /// ```
    /// use tinsel::anim::Glider;
    ///
    /// let g = Glider::new();
    /// assert_eq!(g.step(5.0, 20.0, 1.0), 5.0);
    /// assert!(g.step(25.0, 20.0, 1.0) < 25.0);
    /// assert!(g.moving(1.1));
    /// assert_eq!(g.step(25.0, 20.0, 2.0), 25.0);
    /// ```
    pub fn step(&self, to: f64, span: f64, t: f64) -> f64 {
        let before = self.seen.replace(Some((to, span)));
        let running = self.glide.get().filter(|g| g.moving(t));
        match before {
            Some((was, was_span)) if was_span == span && was != to => {
                let from = running.map_or(was, |g| g.at(t));
                let distance = (to - from).abs();
                let far = (distance / span.max(1.0)).min(1.0);
                let length = self.short + (self.long - self.short) * far;
                self.glide
                    .set((distance > self.snap).then(|| Glide::new(from, to, t - LEAD, length)));
            }
            Some((_, was_span)) if was_span == span => self.glide.set(running),
            _ => self.glide.set(None),
        }
        self.glide.get().map_or(to, |g| g.at(t))
    }

    /// Where the value shows in `frame`, now that it should be `to`, as [`step`](Glider::step)
    /// says, asking the frame for another while it moves. At time 0 it shows where it
    /// lands.
    pub fn follow(&self, to: f64, span: f64, frame: &mut Frame) -> f64 {
        let t = frame.time();
        if t <= 0.0 {
            return self.jump(to, span);
        }
        let shown = self.step(to, span, t);
        if self.moving(t) {
            frame.animate();
        }
        shown
    }

    /// Show `to` straight away, and glide from there next time it changes: for when gliding
    /// is turned off.
    pub fn jump(&self, to: f64, span: f64) -> f64 {
        self.seen.set(Some((to, span)));
        self.glide.set(None);
        to
    }

    /// Forget what it's seen, so the next value shows straight away: for a list showing
    /// something else entirely.
    pub fn reset(&self) {
        self.seen.set(None);
        self.glide.set(None);
    }

    /// The glide it's on, if any, finished or not.
    pub fn glide(&self) -> Option<Glide> {
        self.glide.get()
    }

    /// Whether it's still gliding at `t` seconds.
    pub fn moving(&self, t: f64) -> bool {
        self.glide.get().is_some_and(|g| g.moving(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_eases_out_and_stays_put_either_side() {
        let g = Glide::new(10.0, 0.0, 2.0, 1.0);
        assert_eq!(g.at(1.0), 10.0);
        assert_eq!(g.at(3.5), 0.0);
        let (a, b, c) = (g.at(2.25), g.at(2.5), g.at(2.75));
        assert!(10.0 - a > a - b && a - b > b - c, "quick, then slower: {a} {b} {c}");
        assert!(g.moving(2.9) && !g.moving(3.0));
        assert_eq!((g.from(), g.to()), (10.0, 0.0));
        // no time at all is a jump
        let jump = Glide::new(0.0, 4.0, 1.0, 0.0);
        assert_eq!((jump.at(0.5), jump.at(1.0)), (0.0, 4.0));
    }

    #[test]
    fn a_glider_glides_on_from_wherever_it_is() {
        let g = Glider::new().duration(0.2, 0.2);
        g.step(0.0, 10.0, 1.0);
        let mid = g.step(10.0, 10.0, 1.1);
        assert!(mid > 0.0 && mid < 10.0);
        // turned back halfway: it starts from where it shows, not from where it was going
        let back = g.step(0.0, 10.0, 1.1);
        assert_eq!(g.glide().unwrap().from(), mid);
        assert!(back <= mid);
        assert_eq!(g.step(0.0, 10.0, 2.0), 0.0);
        assert!(!g.moving(2.0));
    }

    #[test]
    fn small_moves_and_new_spans_jump() {
        let g = Glider::new().snap(2.0);
        g.step(0.0, 10.0, 1.0);
        assert_eq!(g.step(2.0, 10.0, 1.1), 2.0, "near enough to snap");
        assert_eq!(g.step(9.0, 20.0, 1.2), 9.0, "the span changed");
        assert!(g.step(19.0, 20.0, 1.3) < 19.0);
        g.reset();
        assert_eq!(g.step(0.0, 20.0, 1.31), 0.0);
        assert_eq!(g.jump(50.0, 20.0), 50.0);
        assert!(g.step(30.0, 20.0, 1.4) > 30.0, "glides on from the jump");
    }

    #[test]
    fn longer_moves_take_longer() {
        let g = Glider::new();
        g.step(0.0, 10.0, 0.0);
        g.step(3.0, 10.0, 1.0);
        let short = g.glide().unwrap().end();
        g.step(3.0, 10.0, 5.0);
        g.step(103.0, 10.0, 6.0);
        let long = g.glide().unwrap().end() - 5.0;
        assert!(long > short, "{long} {short}");
    }

    #[test]
    fn following_in_a_frame_asks_for_frames_until_it_lands() {
        let g = Glider::new();
        let mut f = Frame::new(1, 1).at(1.0);
        g.follow(0.0, 10.0, &mut f);
        let mut f = Frame::new(1, 1).at(1.01);
        g.follow(10.0, 10.0, &mut f);
        assert!(f.wants_redraw());
        let mut f = Frame::new(1, 1);
        assert_eq!(g.follow(20.0, 10.0, &mut f), 20.0, "at time 0 it lands");
        assert!(!f.wants_redraw());
    }
}

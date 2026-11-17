//! When the next frame is due. An app draws when something happens (a key, a wake, a resize,
//! focus coming or going), and otherwise only when the last frame asked for another.

/// What a frame asked of the frames after it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Wants {
    /// Something is moving: the next frame as soon as the app's frame rate allows.
    pub(crate) motion: bool,
    /// Something drifts in the background, at this many frames a second; paused while the
    /// terminal is in the background.
    pub(crate) ambient: Option<u32>,
    /// The earliest moment, on the app's clock, the frame asked to be drawn again.
    pub(crate) at: Option<f64>,
}

impl Wants {
    pub(crate) fn animate(&mut self) {
        self.motion = true;
    }

    pub(crate) fn animate_at(&mut self, fps: u32) {
        let fps = fps.clamp(1, 240);
        self.ambient = Some(self.ambient.map_or(fps, |was| was.max(fps)));
    }

    pub(crate) fn redraw_at(&mut self, seconds: f64) {
        if seconds.is_nan() {
            return;
        }
        self.at = Some(self.at.map_or(seconds, |was| was.min(seconds)));
    }

    /// Whether anything at all was asked for.
    pub(crate) fn any(&self) -> bool {
        self.motion || self.ambient.is_some() || self.at.is_some()
    }
}

/// When to draw next, on the app's clock, after a frame drawn at `drawn` that asked for
/// `wants`, in an app drawing at most `fps` frames a second. `None` means nothing was asked
/// for, so the app waits for something to happen. While the terminal is in the background
/// (`focused` false) ambient animation waits too; motion carries on, so it can finish.
pub(crate) fn next_frame(wants: Wants, drawn: f64, fps: u32, focused: bool) -> Option<f64> {
    let step = |fps: u32| 1.0 / f64::from(fps.clamp(1, 240));
    let full = step(fps);
    let motion = wants.motion.then_some(drawn + full);
    let ambient = wants
        .ambient
        .filter(|_| focused)
        .map(|rate| drawn + step(rate).max(full));
    [motion, ambient, wants.at].into_iter().flatten().reduce(f64::min)
}

/// The ambient clock: the app's clock, standing still while the terminal is in the
/// background, so ambient animation carries on from where it was when focus comes back
/// rather than jumping ahead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Ambient {
    /// Seconds spent in the background so far, not counting now.
    away: f64,
    /// When focus went, while it's gone.
    left: Option<f64>,
}

impl Ambient {
    /// The terminal gained (`true`) or lost focus at `now`.
    pub(crate) fn focus(&mut self, on: bool, now: f64) {
        match (on, self.left) {
            (false, None) => self.left = Some(now),
            (true, Some(left)) => {
                self.away += (now - left).max(0.0);
                self.left = None;
            }
            _ => {}
        }
    }

    pub(crate) fn focused(&self) -> bool {
        self.left.is_none()
    }

    /// The ambient clock when the app's clock reads `now`.
    pub(crate) fn at(&self, now: f64) -> f64 {
        self.left.unwrap_or(now).min(now) - self.away
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_asked_for_waits_for_something_to_happen() {
        assert_eq!(next_frame(Wants::default(), 3.0, 60, true), None);
        assert!(!Wants::default().any());
    }

    #[test]
    fn motion_comes_at_the_full_rate_focused_or_not() {
        let mut wants = Wants::default();
        wants.animate();
        assert_eq!(next_frame(wants, 1.0, 50, true), Some(1.02));
        assert_eq!(next_frame(wants, 1.0, 50, false), Some(1.02));
        // kept in range
        assert_eq!(next_frame(wants, 0.0, 0, true), Some(1.0));
    }

    #[test]
    fn ambient_animation_runs_slower_and_rests_in_the_background() {
        let mut wants = Wants::default();
        wants.animate_at(10);
        wants.animate_at(4);
        assert_eq!(wants.ambient, Some(10), "the fastest asked for wins");
        assert_eq!(next_frame(wants, 2.0, 60, true), Some(2.1));
        assert_eq!(next_frame(wants, 2.0, 60, false), None);
        // never quicker than the app draws at all
        wants.animate_at(120);
        assert_eq!(next_frame(wants, 0.0, 20, true), Some(0.05));
        // motion and ambient together: whichever comes first
        wants.animate();
        assert_eq!(next_frame(wants, 0.0, 20, false), Some(0.05));
    }

    #[test]
    fn a_moment_asked_for_comes_even_in_the_background() {
        let mut wants = Wants::default();
        wants.redraw_at(60.0);
        wants.redraw_at(f64::NAN);
        wants.redraw_at(90.0);
        assert_eq!(wants.at, Some(60.0), "the earliest asked for wins");
        assert_eq!(next_frame(wants, 1.0, 60, false), Some(60.0));
        wants.animate_at(1);
        assert_eq!(next_frame(wants, 1.0, 60, true), Some(2.0));
        assert_eq!(next_frame(wants, 1.0, 60, false), Some(60.0));
        // a moment already gone is due at once
        let mut late = Wants::default();
        late.redraw_at(0.5);
        assert_eq!(next_frame(late, 1.0, 60, true), Some(0.5));
    }

    #[test]
    fn the_ambient_clock_stands_still_in_the_background() {
        let mut clock = Ambient::default();
        assert_eq!(clock.at(2.0), 2.0);
        clock.focus(false, 3.0);
        assert!(!clock.focused());
        assert_eq!(clock.at(10.0), 3.0);
        // lost twice is still lost from the first time
        clock.focus(false, 5.0);
        assert_eq!(clock.at(10.0), 3.0);
        clock.focus(true, 10.0);
        assert!(clock.focused());
        assert_eq!(clock.at(10.0), 3.0);
        assert_eq!(clock.at(11.5), 4.5);
        clock.focus(true, 12.0);
        assert_eq!(clock.at(12.0), 5.0);
    }
}

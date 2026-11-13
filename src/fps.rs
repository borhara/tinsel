//! How smoothly things move.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Frames a second unless [`set_fps`] or `TINSEL_FPS` says otherwise.
pub const DEFAULT_FPS: u32 = 60;

// 0 = not decided yet
static FPS: AtomicU32 = AtomicU32::new(0);

/// Set how many frames a second every animation draws: spinners, progress bars, [`Live`]
/// displays and full-screen apps. 60 unless set, or `TINSEL_FPS` set; one display can still
/// choose its own ([`Live::fps`], [`Progress::fps`], [`App::fps`]). Animations are timed by
/// the clock, so this changes how smooth they look, not how fast they go. Kept between 1
/// and 240.
///
/// [`Live`]: crate::Live
/// [`Live::fps`]: crate::Live::fps
/// [`Progress::fps`]: crate::Progress::fps
/// [`App::fps`]: crate::app::App::fps
///
/// ```
/// tinsel::set_fps(30);
/// assert_eq!(tinsel::fps(), 30);
/// ```
pub fn set_fps(fps: u32) {
    FPS.store(fps.clamp(1, 240), Ordering::Relaxed);
}

/// The frames a second animations draw at, as [`set_fps`] describes.
pub fn fps() -> u32 {
    match FPS.load(Ordering::Relaxed) {
        0 => {
            let fps = parse_fps(std::env::var("TINSEL_FPS").ok().as_deref());
            FPS.store(fps, Ordering::Relaxed);
            fps
        }
        fps => fps,
    }
}

fn parse_fps(env: Option<&str>) -> u32 {
    env.and_then(|v| v.trim().parse::<u32>().ok())
        .map_or(DEFAULT_FPS, |f| f.clamp(1, 240))
}

/// How long one frame lasts at `fps` frames a second (0 meaning the global rate).
pub(crate) fn frame(fps: u32) -> Duration {
    let fps = if fps == 0 { self::fps() } else { fps };
    Duration::from_secs(1) / fps.clamp(1, 240)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_lasts_a_share_of_a_second_kept_in_range() {
        assert_eq!(frame(120), Duration::from_secs(1) / 120);
        assert_eq!(frame(1000), Duration::from_secs(1) / 240);
        assert_eq!(frame(0), Duration::from_secs(1) / fps());
    }
}

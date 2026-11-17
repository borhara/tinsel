//! The pieces tinsel animates with, for drawing your own: a light passing over a label, the
//! twinkle a finished step shows, and bars still on the move. Each takes the seconds since
//! the animation started, so it can be drawn at any moment, as often as you like.
//!
//! ```
//! use tinsel::anim;
//!
//! let t = 1.5;
//! println!("{} uploading {}", tinsel::Spinner::Sparkle.frame(t), anim::bar(0.4, 20, t));
//! println!("{}", anim::shimmer("thinking", t));
//! ```

pub use crate::widgets::{live_bar as bar, live_waveform as waveform, pop, shimmer};

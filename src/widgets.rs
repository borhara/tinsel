use std::fmt;

use crate::block::Block;
use crate::style::palette::{BERRY, BLUSH, LINE, MUTED, PETAL, RED, ROSE, SHINE, SOFT, TEXT, TRACK};
use crate::style::{Line, RAMP, Rgb, mix, paint, sample};
use crate::term::content_width;

const ARC: [&str; 6] = ["◜", "◠", "◝", "◞", "◡", "◟"];
const ORBIT: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
const STARS: [&str; 4] = ["·", "⋆", "✧", "✦"];
const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Random-looking but repeatable 0..1 noise for a cell at a moment, so glitter flickers
/// without needing a random number generator.
pub(crate) fn noise(cell: u32, tick: u32) -> f32 {
    let mut x = cell.wrapping_mul(0x9E37_79B1) ^ tick.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// How a step looks while it's running. Every style keeps to the pink palette and
/// eases its brightness rather than snapping, so none of them feel mechanical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spinner {
    /// Three little stars twinkling out of step - the tinsel one.
    #[default]
    Sparkle,
    /// Three dots with a brightness wave rolling through them, like a typing indicator.
    Dots,
    /// A star gliding back and forth along a short track, leaving a fading trail.
    Comet,
    /// Five bars rising and falling like a live audio waveform, brighter as they peak.
    Wave,
    /// A single arc turning smoothly.
    Arc,
    /// A gap travelling around a braille ring, its colour drifting berry → blush.
    Orbit,
    /// One dot slowly breathing in size and colour, flashing a star at its brightest.
    Pulse,
}

impl Spinner {
    /// Every spinner, in the order the showcase lists them.
    pub const ALL: [Spinner; 7] = [
        Spinner::Sparkle,
        Spinner::Dots,
        Spinner::Comet,
        Spinner::Wave,
        Spinner::Arc,
        Spinner::Orbit,
        Spinner::Pulse,
    ];

    /// The spinner's lowercase name, as used by `--spinner` flags.
    pub fn name(self) -> &'static str {
        match self {
            Spinner::Sparkle => "sparkle",
            Spinner::Dots => "dots",
            Spinner::Comet => "comet",
            Spinner::Wave => "wave",
            Spinner::Arc => "arc",
            Spinner::Orbit => "orbit",
            Spinner::Pulse => "pulse",
        }
    }

    /// The spinner with this name, if there is one: handy for a `--spinner` flag.
    pub fn from_name(name: &str) -> Option<Spinner> {
        Spinner::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Columns the spinner occupies; step marks are centred in the same space so labels never shift.
    pub fn width(self) -> usize {
        match self {
            Spinner::Sparkle | Spinner::Dots => 3,
            Spinner::Comet | Spinner::Wave => 5,
            _ => 1,
        }
    }

    /// What the spinner looks like `t` seconds in. Call it every frame with the time so far.
    pub fn frame(self, t: f32) -> Line {
        let rest = mix(TRACK, BERRY, 0.45);
        match self {
            Spinner::Sparkle => {
                const PHASE: [f32; 3] = [0.0, 2.1, 4.2];
                const SPEED: [f32; 3] = [4.6, 6.1, 3.9];
                (0..3).fold(Line::new(), |line, i| {
                    let b = ((t * SPEED[i] + PHASE[i]).sin() + 1.0) / 2.0;
                    // squared so each star rests dim and only briefly flares
                    let b = b * b;
                    line.push(STARS[((b * 4.0) as usize).min(3)], sample(&[rest, ROSE, SHINE], b))
                })
            }
            Spinner::Dots => (0..3).fold(Line::new(), |line, i| {
                let v = (t * 5.0 - i as f32 * 0.9).sin().max(0.0).powi(2);
                line.push("•", sample(&[rest, ROSE, PETAL], v))
            }),
            Spinner::Wave => (0..5).fold(Line::new(), |line, i| {
                let x = i as f32;
                // two waves at unrelated speeds, so the shape keeps changing like real audio
                let h = 0.5 + 0.3 * (t * 6.0 - x * 0.9).sin() + 0.2 * (t * 9.7 + x * 1.7).sin();
                let h = h.clamp(0.0, 1.0);
                line.push(
                    LEVELS[((h * 7.0).round() as usize).min(7)],
                    sample(&[rest, ROSE, SHINE], h * h),
                )
            }),
            Spinner::Comet => {
                // triangle wave across the track, eased at the ends so the turn feels soft
                let phase = (t * 0.9).rem_euclid(2.0);
                let x = if phase > 1.0 { 2.0 - phase } else { phase };
                let eased = x * x * (3.0 - 2.0 * x);
                let head = eased * 4.0;
                let dir = if phase > 1.0 { -1.0 } else { 1.0 };
                (0..5).fold(Line::new(), |line, i| {
                    let d = i as f32 - head;
                    let v = if d * dir <= 0.0 {
                        (-d.abs() / 1.1).exp()
                    } else {
                        (-d.abs() * 3.0).exp()
                    };
                    let glyph = if v > 0.9 {
                        "✦"
                    } else if v > 0.6 {
                        "●"
                    } else if v > 0.25 {
                        "•"
                    } else {
                        "·"
                    };
                    line.push(glyph, sample(&[rest, ROSE, SHINE], v))
                })
            }
            Spinner::Arc => Line::new().push(ARC[(t * 12.0) as usize % ARC.len()], ROSE),
            Spinner::Orbit => {
                let drift = ((t * 0.8).sin() + 1.0) / 2.0;
                Line::new().push(ORBIT[(t * 12.0) as usize % ORBIT.len()], sample(&RAMP, drift))
            }
            Spinner::Pulse => {
                let v = ((t * 3.2).sin() + 1.0) / 2.0;
                let glyph = if v > 0.92 {
                    "✦"
                } else if v > 0.66 {
                    "●"
                } else if v > 0.33 {
                    "•"
                } else {
                    "∙"
                };
                Line::new().push(glyph, sample(&[BERRY, ROSE, SHINE], v))
            }
        }
    }
}

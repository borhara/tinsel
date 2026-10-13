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

/// A soft light passing slowly across the label, with a little glitter: letters near the
/// light now and then catch it and flash.
pub fn shimmer(label: &str, t: f32) -> Line {
    let n = label.chars().count() as f32;
    let pos = (t * 14.0).rem_euclid(n + 24.0) - 12.0;
    let tick = (t * 7.0) as u32;
    label.chars().enumerate().fold(Line::new(), |line, (j, c)| {
        let k = (-(j as f32 - pos).powi(2) / 12.0).exp();
        let glitter = noise(j as u32, tick) < 0.015 + 0.2 * k;
        let color = if glitter { SHINE } else { mix(SOFT, PETAL, k) };
        line.push(c.encode_utf8(&mut [0; 4]), color)
    })
}

/// The mark a step shows in the moment after it finishes: a quick twinkle that settles into
/// `●`. `None` once settled.
pub fn pop(age: f32) -> Option<Line> {
    let (glyph, color) = match age {
        a if a < 0.1 => ("✧", SHINE),
        a if a < 0.22 => ("✦", SHINE),
        a if a < 0.34 => ("✦", PETAL),
        a if a < 0.46 => ("✧", BLUSH),
        _ => return None,
    };
    Some(Line::new().push(glyph, color))
}

/// A thin line that fills berry → blush, with half-cell precision.
pub fn bar(fraction: f32, width: usize) -> Line {
    shiny_bar(fraction, width, None)
}

/// [`bar`] with a soft glint running along the filled part, for bars that are still moving.
pub fn live_bar(fraction: f32, width: usize, t: f32) -> Line {
    shiny_bar(fraction, width, Some(t))
}

fn shiny_bar(fraction: f32, width: usize, t: Option<f32>) -> Line {
    let halves = (fraction.clamp(0.0, 1.0) * width as f32 * 2.0).round() as usize;
    let filled = halves.div_ceil(2) as f32;
    let glint = t.map(|t| (t * 16.0).rem_euclid(filled + 10.0) - 5.0);
    (0..width).fold(Line::new(), |line, i| {
        let base = sample(&RAMP, i as f32 / width.saturating_sub(1).max(1) as f32);
        let c = glint.map_or(base, |g| mix(base, SHINE, (-(i as f32 - g).powi(2) / 3.0).exp() * 0.75));
        match halves.saturating_sub(i * 2) {
            0 => line.push("━", TRACK),
            1 => line.push("╸", c),
            _ => line.push("━", c),
        }
    })
}

/// How a progress bar is drawn, for [`Live::with_bar`](crate::Live::with_bar) and
/// [`Progress::with_bar`](crate::Progress::with_bar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarStyle {
    /// A thin line with a glint running along it: [`bar`] and [`live_bar`]. The default.
    #[default]
    Line,
    /// A row of bars like an audio track, lit up as it plays through: [`waveform`] and
    /// [`live_waveform`].
    Waveform,
}

impl BarStyle {
    /// The bar at `fraction`, `t` seconds into its animation.
    pub(crate) fn draw(self, fraction: f32, width: usize, t: f32) -> Line {
        match self {
            BarStyle::Line => live_bar(fraction, width, t),
            BarStyle::Waveform => live_waveform(fraction, width, t),
        }
    }
}

/// How tall the waveform is at column `i`, 0..1: a few slow waves at unrelated speeds plus a
/// little grain, so it looks like a recording but is the same shape every time it's drawn.
fn wave_height(i: usize) -> f32 {
    let x = i as f32;
    let h = 0.45 + 0.3 * (x * 0.55).sin() + 0.18 * (x * 1.7 + 1.3).sin() + 0.1 * (x * 0.13).sin();
    // squared, so most columns sit low and a few peak, like speech or music
    let h = (h + 0.3 * (noise(i as u32, 7) - 0.5)).clamp(0.0, 1.0);
    (h * h * 1.3).clamp(0.08, 1.0)
}

/// Progress as an audio waveform: played columns in berry → blush, the rest dim.
///
/// ```
/// println!("{}", tinsel::waveform(0.4, 30));
/// ```
pub fn waveform(fraction: f32, width: usize) -> Line {
    wave(fraction, width, None)
}

/// [`waveform`] still playing: the bars just behind the playhead bounce, and the playhead
/// itself glows.
pub fn live_waveform(fraction: f32, width: usize, t: f32) -> Line {
    wave(fraction, width, Some(t))
}

fn wave(fraction: f32, width: usize, t: Option<f32>) -> Line {
    let head = fraction.clamp(0.0, 1.0) * width as f32;
    (0..width).fold(Line::new(), |line, i| {
        let x = i as f32;
        let mut h = wave_height(i);
        // how much of this column has been played, so the edge moves smoothly between cells
        let played = (head - x).clamp(0.0, 1.0);
        if let Some(t) = t.filter(|_| played > 0.0 && head < width as f32) {
            // the last few played columns dance, fading with distance from the head
            let near = (-(head - x - 0.5).max(0.0) / 2.5).exp();
            h = (h + 0.3 * near * (t * 11.0 + x * 1.9).sin()).clamp(0.12, 1.0);
        }
        let glyph = LEVELS[((h * 7.0).round() as usize).min(7)];
        if played <= 0.0 {
            return line.push(glyph, TRACK);
        }
        let base = sample(&RAMP, x / width.saturating_sub(1).max(1) as f32);
        let lit = mix(TRACK, base, played);
        let glow = t.map_or(0.0, |_| (-(head - x - 0.5).abs() / 0.8).exp() * 0.7);
        line.push(glyph, mix(lit, SHINE, glow))
    })
}

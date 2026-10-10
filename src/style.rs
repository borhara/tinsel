use std::fmt;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A colour as red, green and blue, 0-255 each.
pub struct Rgb(pub u8, pub u8, pub u8);

/// The palette everything in tinsel is drawn from: pinks for accent, quiet greys for the rest.
pub mod palette {
    use super::Rgb;

    /// Deep pink: the dark end of the ramp.
    pub const BERRY: Rgb = Rgb(190, 80, 124);
    /// The accent: done marks, headings, the spinner's rest colour.
    pub const ROSE: Rgb = Rgb(236, 122, 164);
    /// Light pink: secondary accents and values.
    pub const BLUSH: Rgb = Rgb(246, 172, 200);
    /// Palest pink: the peak of the shimmer.
    pub const PETAL: Rgb = Rgb(252, 218, 231);
    /// The brightest point of a glint or a twinkle - near white, pinked.
    pub const SHINE: Rgb = Rgb(255, 242, 248);
    /// Main text: a warm, pinkish white.
    pub const TEXT: Rgb = Rgb(236, 230, 234);
    /// Secondary text: details and notes.
    pub const SOFT: Rgb = Rgb(172, 160, 168);
    /// Quiet text: timings, keys, hints.
    pub const MUTED: Rgb = Rgb(118, 106, 114);
    /// Card borders: a grey with a hint of rose.
    pub const LINE: Rgb = Rgb(120, 92, 106);
    /// Unfilled progress track.
    pub const TRACK: Rgb = Rgb(66, 58, 64);
    /// Green, for additions and tips.
    pub const SAGE: Rgb = Rgb(140, 200, 160);
    /// Amber, for warnings.
    pub const AMBER: Rgb = Rgb(240, 184, 100);
    /// Red, for errors and removals.
    pub const RED: Rgb = Rgb(240, 96, 96);
}

/// Berry → rose → blush, for titles and filled progress.
pub const RAMP: [Rgb; 3] = [palette::BERRY, palette::ROSE, palette::BLUSH];

/// How much colour tinsel draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    /// Plain text, no escape codes at all: piped output, `NO_COLOR`, or tests.
    None,
    /// The 256-colour palette, for terminals that don't advertise truecolor.
    Ansi256,
    /// Full 24-bit colour.
    TrueColor,
}

// 0 = not decided yet; otherwise 1 + the ColorMode's position
static MODE: AtomicU8 = AtomicU8::new(0);

/// The colour mode in use. Decided once, from `NO_COLOR`, whether stdout is a terminal, and
/// `COLORTERM`, unless [`set_color_mode`] has chosen one.
pub fn color_mode() -> ColorMode {
    match MODE.load(Ordering::Relaxed) {
        1 => ColorMode::None,
        2 => ColorMode::Ansi256,
        3 => ColorMode::TrueColor,
        _ => {
            let mode = detect();
            set_color_mode(mode);
            mode
        }
    }
}

/// Choose the colour mode yourself, for a `--color` flag or to get plain text in tests.
///
/// ```
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// assert_eq!(tinsel::paint(tinsel::palette::ROSE, "hi"), "hi");
/// ```
pub fn set_color_mode(mode: ColorMode) {
    let code = match mode {
        ColorMode::None => 1,
        ColorMode::Ansi256 => 2,
        ColorMode::TrueColor => 3,
    };
    MODE.store(code, Ordering::Relaxed);
}

fn detect() -> ColorMode {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    if no_color || !std::io::stdout().is_terminal() {
        return ColorMode::None;
    }
    crate::term::enable_escapes();
    match std::env::var("COLORTERM") {
        Ok(v) if v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit") => ColorMode::TrueColor,
        _ => ColorMode::Ansi256,
    }
}

/// Whether the terminal's background is dark or light. tinsel's palette is designed for dark
/// backgrounds; on light ones every colour is redrawn darker, keeping its hue, so pinks stay
/// pink and text stays readable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// Light text on a dark background.
    Dark,
    /// Dark text on a light background.
    Light,
}

// 0 = not decided yet, 1 = dark, 2 = light
static THEME: AtomicU8 = AtomicU8::new(0);

/// The theme in use. Decided once: from `TINSEL_THEME` (`light` or `dark`) if set, then the
/// `COLORFGBG` variable some terminals export, then by asking the terminal for its background
/// colour. Falls back to [`Theme::Dark`].
pub fn theme() -> Theme {
    match THEME.load(Ordering::Relaxed) {
        1 => Theme::Dark,
        2 => Theme::Light,
        _ => {
            let theme = detect_theme();
            set_theme(theme);
            theme
        }
    }
}

/// Choose the theme yourself, for a `--theme` flag or to preview the other one.
pub fn set_theme(theme: Theme) {
    THEME.store(if theme == Theme::Light { 2 } else { 1 }, Ordering::Relaxed);
}

fn detect_theme() -> Theme {
    match std::env::var("TINSEL_THEME").as_deref() {
        Ok("light") => return Theme::Light,
        Ok("dark") => return Theme::Dark,
        _ => {}
    }
    // "fg;bg" in the 16-colour palette: 7 and 9-15 are light backgrounds
    if let Some(bg) = std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok())
    {
        return if bg == 7 || bg > 8 { Theme::Light } else { Theme::Dark };
    }
    if color_mode() == ColorMode::None {
        return Theme::Dark;
    }
    match crate::term::query_background() {
        Some((r, g, b)) if luminance(Rgb(r, g, b)) > 0.5 => Theme::Light,
        _ => Theme::Dark,
    }
}

fn luminance(Rgb(r, g, b): Rgb) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

/// A colour redrawn for a light background: same hue, a touch more saturated, and its
/// lightness flipped so what was bright on dark (text, glints) is now the strongest contrast.
fn for_light(rgb: Rgb) -> Rgb {
    let (h, s, l) = to_hsl(rgb);
    from_hsl(h, (s * 1.1).min(1.0), 0.15 + (1.0 - l) * 0.75)
}

fn to_hsl(Rgb(r, g, b): Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Rgb {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return Rgb(v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round() as u8
    };
    Rgb(channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
}

/// Blend two colours: `t = 0` is all `a`, `t = 1` is all `b`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

/// Sample a multi-stop gradient at t in 0..=1.
pub fn sample(stops: &[Rgb], t: f32) -> Rgb {
    match stops.len() {
        0 => palette::TEXT,
        1 => stops[0],
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x.floor() as usize).min(n - 2);
            mix(stops[i], stops[i + 1], x - i as f32)
        }
    }
}

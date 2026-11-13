use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
/// Whether MODE was worked out rather than chosen, so it can be worked out again for another stream.
static DETECTED: AtomicBool = AtomicBool::new(false);

/// The colour mode in use. Decided once, from `NO_COLOR`, whether stdout (or the
/// [`Target`](crate::Target)) is a terminal, and
/// `COLORTERM`, unless [`set_color_mode`] has chosen one.
pub fn color_mode() -> ColorMode {
    #[cfg(test)]
    if let Some(mode) = TEST_MODE.get() {
        return mode;
    }
    match MODE.load(Ordering::Relaxed) {
        1 => ColorMode::None,
        2 => ColorMode::Ansi256,
        3 => ColorMode::TrueColor,
        _ => {
            let mode = detect();
            set_color_mode(mode);
            DETECTED.store(true, Ordering::Relaxed);
            mode
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_MODE: std::cell::Cell<Option<ColorMode>> = const { std::cell::Cell::new(None) };
    static TEST_THEME: std::cell::Cell<Option<Theme>> = const { std::cell::Cell::new(None) };
}

/// Run `f` in a colour mode on this thread alone, so a unit test that wants colour doesn't
/// turn it on for the tests running beside it.
#[cfg(test)]
pub(crate) fn with_color_mode<T>(mode: ColorMode, f: impl FnOnce() -> T) -> T {
    let before = TEST_MODE.replace(Some(mode));
    let out = f();
    TEST_MODE.set(before);
    out
}

/// Run `f` in a theme on this thread alone.
#[cfg(test)]
pub(crate) fn with_theme<T>(theme: Theme, f: impl FnOnce() -> T) -> T {
    let before = TEST_THEME.replace(Some(theme));
    let out = f();
    TEST_THEME.set(before);
    out
}

/// Decide the colour mode again next time, unless it was chosen with [`set_color_mode`].
pub(crate) fn forget_detected() {
    if DETECTED.swap(false, Ordering::Relaxed) {
        MODE.store(0, Ordering::Relaxed);
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
    DETECTED.store(false, Ordering::Relaxed);
    MODE.store(code, Ordering::Relaxed);
}

fn detect() -> ColorMode {
    let no_color = !std::env::var_os("NO_COLOR").unwrap_or_default().is_empty();
    let colorterm = std::env::var("COLORTERM").ok();
    pick_color_mode(no_color, crate::output::is_terminal(), colorterm.as_deref())
}

fn pick_color_mode(no_color: bool, terminal: bool, colorterm: Option<&str>) -> ColorMode {
    if no_color || !terminal {
        return ColorMode::None;
    }
    crate::term::enable_escapes();
    match colorterm {
        Some(v) if v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit") => ColorMode::TrueColor,
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
    #[cfg(test)]
    if let Some(theme) = TEST_THEME.get() {
        return theme;
    }
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
    let (name, colorfgbg) = (std::env::var("TINSEL_THEME").ok(), std::env::var("COLORFGBG").ok());
    pick_theme(name.as_deref(), colorfgbg.as_deref(), crate::term::query_background)
}

/// The theme from `TINSEL_THEME`, then `COLORFGBG`, then the terminal's background colour.
fn pick_theme(name: Option<&str>, colorfgbg: Option<&str>, background: fn() -> Option<(u8, u8, u8)>) -> Theme {
    match name {
        Some("light") => return Theme::Light,
        Some("dark") => return Theme::Dark,
        _ => {}
    }
    // "fg;bg" in the 16-colour palette: 7 and 9-15 are light backgrounds
    if let Some(bg) = colorfgbg.and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok()) {
        return if bg == 7 || bg > 8 { Theme::Light } else { Theme::Dark };
    }
    if color_mode() == ColorMode::None {
        return Theme::Dark;
    }
    match background() {
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

/// The colour family tinsel's accents are drawn in. Pink is tinsel's own; the others redraw
/// every pink (accents, the spinner, the faint tint in the greys) in another hue, so a CLI can
/// match its brand. Red, amber and sage keep their meaning whatever the accent.
///
/// ```
/// tinsel::set_accent(tinsel::Accent::Blue);
/// # tinsel::set_accent(tinsel::Accent::Pink);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Accent {
    /// tinsel's own: rose, blush and berry.
    #[default]
    Pink,
    /// Lavender and plum.
    Violet,
    /// Cornflower and navy.
    Blue,
    /// Aqua and deep teal.
    Teal,
    /// Apricot and rust.
    Orange,
    /// Any hue, in degrees around the colour wheel (pink is about 338°).
    Hue(f32),
}

impl Accent {
    /// The accent with this name (`pink`, `violet`, `blue`, `teal`, `orange`), or a hue in degrees.
    pub fn from_name(name: &str) -> Option<Accent> {
        Some(match name {
            "pink" => Accent::Pink,
            "violet" | "purple" => Accent::Violet,
            "blue" => Accent::Blue,
            "teal" => Accent::Teal,
            "orange" => Accent::Orange,
            n => Accent::Hue(n.parse().ok()?),
        })
    }

    /// The hue the accent sits at, in degrees.
    fn hue(self) -> f32 {
        match self {
            Accent::Pink => PINK_HUE,
            Accent::Violet => 275.0,
            Accent::Blue => 220.0,
            Accent::Teal => 178.0,
            Accent::Orange => 24.0,
            Accent::Hue(h) => h.rem_euclid(360.0),
        }
    }
}

/// Where tinsel's pinks sit on the colour wheel.
const PINK_HUE: f32 = 338.0;

// the accent's hue in degrees as f32 bits; 0 = not decided yet
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The accent in use: from `TINSEL_ACCENT` if set, otherwise [`Accent::Pink`], unless
/// [`set_accent`] has chosen one.
fn accent_hue() -> f32 {
    match ACCENT.load(Ordering::Relaxed) {
        0 => {
            let name = std::env::var("TINSEL_ACCENT").ok();
            let accent = name.as_deref().and_then(Accent::from_name).unwrap_or_default();
            set_accent(accent);
            accent.hue()
        }
        bits => f32::from_bits(bits),
    }
}

/// Choose the accent colour, for a brand or a `--accent` flag.
pub fn set_accent(accent: Accent) {
    // a hue of exactly 0.0 has bits 0, which means "undecided", so nudge it
    let hue = accent.hue().max(f32::MIN_POSITIVE);
    ACCENT.store(hue.to_bits(), Ordering::Relaxed);
}

/// A colour in the accent's family. Only pinks (hues from magenta to just short of red) move;
/// everything else, red included, stays put.
fn for_accent(rgb: Rgb, hue: f32) -> Rgb {
    let (h, s, l) = to_hsl(rgb);
    let deg = h * 360.0;
    if s == 0.0 || !(295.0..354.0).contains(&deg) {
        return rgb;
    }
    from_hsl((deg + hue - PINK_HUE).rem_euclid(360.0) / 360.0, s, l)
}

/// A palette colour as it's drawn, in the accent and theme in use.
pub(crate) fn shown(rgb: Rgb) -> Rgb {
    let hue = accent_hue();
    let rgb = if (hue - PINK_HUE).abs() > 0.5 {
        for_accent(rgb, hue)
    } else {
        rgb
    };
    if theme() == Theme::Light { for_light(rgb) } else { rgb }
}

/// The SGR parameters for a colour; `layer` is 38 for foreground, 48 for background.
pub(crate) fn sgr(rgb: Rgb, layer: u8) -> String {
    let Rgb(r, g, b) = shown(rgb);
    if color_mode() == ColorMode::TrueColor {
        format!("{layer};2;{r};{g};{b}")
    } else {
        let q = |c: u8| (c as u16 * 5 + 127) / 255;
        format!("{layer};5;{}", 16 + 36 * q(r) + 6 * q(g) + q(b))
    }
}

/// Colour the text. Resets only the foreground, so it nests inside bold, backgrounds and so on.
pub fn paint(rgb: Rgb, s: &str) -> String {
    if color_mode() == ColorMode::None {
        return s.to_string();
    }
    format!("\x1b[{}m{s}\x1b[39m", sgr(rgb, 38))
}

/// Colour both the text and the cells behind it.
pub fn paint_bg(fg: Rgb, bg: Rgb, s: &str) -> String {
    if color_mode() == ColorMode::None {
        return s.to_string();
    }
    format!("\x1b[{};{}m{s}\x1b[39;49m", sgr(fg, 38), sgr(bg, 48))
}

/// Tint the cells behind the text, leaving its own colours alone.
pub(crate) fn on_bg(bg: Rgb, s: &str) -> String {
    if color_mode() == ColorMode::None {
        return s.to_string();
    }
    format!("\x1b[{}m{s}\x1b[49m", sgr(bg, 48))
}

fn attr(on: u8, off: u8, s: &str) -> String {
    if color_mode() == ColorMode::None {
        return s.to_string();
    }
    format!("\x1b[{on}m{s}\x1b[{off}m")
}

/// Bold text.
pub fn bold(s: &str) -> String {
    attr(1, 22, s)
}

/// Faint text.
pub fn dim(s: &str) -> String {
    attr(2, 22, s)
}

/// Italic text, where the terminal's font has it.
pub fn italic(s: &str) -> String {
    attr(3, 23, s)
}

/// Underlined text.
pub fn underline(s: &str) -> String {
    attr(4, 24, s)
}

/// Blinking text, animated by the terminal itself, so it keeps blinking after your program
/// has exited. Most terminals support it (iTerm2, kitty, WezTerm, Ghostty, GNOME Terminal,
/// macOS Terminal); where it's turned off or unsupported, the text simply stays still. Best
/// kept to a single mark: a whole line blinking is hard to read.
///
/// ```
/// println!("  {} ready", tinsel::blink(&tinsel::paint(tinsel::palette::ROSE, "✦")));
/// ```
pub fn blink(s: &str) -> String {
    attr(5, 25, s)
}

/// Colour s across the stops; a non-zero shift slides the gradient back and forth.
pub fn gradient(s: &str, stops: &[Rgb], shift: f32) -> String {
    let n = s.chars().count().saturating_sub(1).max(1) as f32;
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            let t = (i as f32 / n + shift).rem_euclid(2.0);
            paint(
                sample(stops, if t > 1.0 { 2.0 - t } else { t }),
                c.encode_utf8(&mut [0; 4]),
            )
        })
        .collect()
}

/// A painted string that remembers its visible width, so it can be padded and aligned.
#[derive(Clone, Debug, Default)]
pub struct Line {
    text: String,
    width: usize,
}

impl Line {
    /// An empty line.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add text in a colour.
    pub fn push(mut self, s: &str, rgb: Rgb) -> Self {
        self.text += &paint(rgb, s);
        self.width += crate::text::width(s);
        self
    }

    /// Append text that's already styled (a badge, a link); its width is measured without the codes.
    pub fn raw(mut self, styled: &str) -> Self {
        self.text += styled;
        self.width += crate::text::width(styled);
        self
    }

    /// Add another line's text to the end of this one.
    pub fn append(mut self, other: Line) -> Self {
        self.text += &other.text;
        self.width += other.width;
        self
    }

    /// Add `n` spaces.
    pub fn space(mut self, n: usize) -> Self {
        self.text += &" ".repeat(n);
        self.width += n;
        self
    }

    /// A muted, aligned key followed by a value - the row shape cards are built from.
    pub fn kv(key: &str, value: &str, color: Rgb) -> Self {
        Line::new()
            // a column of 10 for the keys, and always a space after a longer one
            .push(&format!("{key:<9} "), palette::MUTED)
            .push(value, color)
    }

    /// How many columns the line takes up on screen.
    pub fn width(&self) -> usize {
        self.width
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_round_trips() {
        for c in [
            palette::ROSE,
            palette::TEXT,
            palette::SAGE,
            palette::TRACK,
            Rgb(0, 0, 0),
            Rgb(255, 255, 255),
        ] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            assert!(
                c.0.abs_diff(back.0) <= 1 && c.1.abs_diff(back.1) <= 1 && c.2.abs_diff(back.2) <= 1,
                "{c:?}"
            );
        }
    }

    #[test]
    fn light_theme_darkens_text_and_keeps_pinks_pink() {
        assert!(luminance(for_light(palette::TEXT)) < 0.3);
        let rose = for_light(palette::ROSE);
        assert!(rose.0 > rose.1 && rose.0 > rose.2, "still reddest: {rose:?}");
        assert!(luminance(rose) < luminance(palette::ROSE));
        // the track goes from dark grey to light grey
        assert!(luminance(for_light(palette::TRACK)) > 0.6);
    }

    #[test]
    fn accents_move_pinks_and_leave_signal_colours() {
        let blue = for_accent(palette::ROSE, 220.0);
        assert!(blue.2 > blue.0, "rose turned blue: {blue:?}");
        assert_eq!(for_accent(palette::RED, 220.0), palette::RED);
        assert_eq!(for_accent(palette::AMBER, 220.0), palette::AMBER);
        assert_eq!(for_accent(palette::SAGE, 220.0), palette::SAGE);
        assert_eq!(Accent::from_name("200"), Some(Accent::Hue(200.0)));
    }

    #[test]
    fn colour_depth_comes_from_the_terminal_and_the_environment() {
        assert_eq!(pick_color_mode(true, true, Some("truecolor")), ColorMode::None);
        assert_eq!(pick_color_mode(false, false, None), ColorMode::None);
        assert_eq!(pick_color_mode(false, true, Some("24bit")), ColorMode::TrueColor);
        assert_eq!(pick_color_mode(false, true, Some("TrueColor")), ColorMode::TrueColor);
        assert_eq!(pick_color_mode(false, true, None), ColorMode::Ansi256);
        assert_eq!(crate::output::with_terminal(false, detect), ColorMode::None);
    }

    #[test]
    fn the_theme_comes_from_its_name_then_the_palette_then_the_background() {
        // the background would say otherwise, so these weren't decided by asking
        let (dark, light) = (|| Some((0, 0, 0)), || Some((255, 255, 255)));
        assert_eq!(pick_theme(Some("light"), Some("0;0"), dark), Theme::Light);
        assert_eq!(pick_theme(Some("dark"), None, light), Theme::Dark);
        assert_eq!(pick_theme(Some("sepia"), Some("0;15"), dark), Theme::Light);
        assert_eq!(pick_theme(None, Some("15;7"), dark), Theme::Light);
        assert_eq!(pick_theme(None, Some("15;0"), light), Theme::Dark);
        with_color_mode(ColorMode::None, || {
            assert_eq!(pick_theme(None, None, light), Theme::Dark)
        });
        with_color_mode(ColorMode::TrueColor, || {
            assert_eq!(pick_theme(None, None, dark), Theme::Dark);
            assert_eq!(pick_theme(None, None, light), Theme::Light);
            assert_eq!(pick_theme(None, None, || Some((250, 250, 250))), Theme::Light);
            assert_eq!(pick_theme(None, None, || Some((20, 20, 20))), Theme::Dark);
            assert_eq!(pick_theme(None, None, || None), Theme::Dark);
        });
        // with no terminal to ask, whatever the environment says it's a theme
        let _ = with_color_mode(ColorMode::TrueColor, detect_theme);
    }

    #[test]
    fn blue_is_a_hue_too() {
        let (h, s, _) = to_hsl(Rgb(0, 0, 255));
        assert!((h * 360.0 - 240.0).abs() < 0.5 && s == 1.0);
    }

    #[test]
    fn gradients_of_one_stop_or_none() {
        assert_eq!(sample(&[], 0.5), palette::TEXT);
        assert_eq!(sample(&[palette::SAGE], 0.5), palette::SAGE);
    }

    #[test]
    fn every_accent_has_a_name_and_a_hue() {
        let hues: Vec<f32> = ["pink", "purple", "violet", "blue", "teal", "orange", "-20"]
            .iter()
            .map(|n| Accent::from_name(n).unwrap().hue())
            .collect();
        assert_eq!(hues, [PINK_HUE, 275.0, 275.0, 220.0, 178.0, 24.0, 340.0]);
        assert_eq!(Accent::from_name("plaid"), None);
    }

    #[test]
    fn colours_fall_back_to_the_256_palette() {
        let red = with_color_mode(ColorMode::Ansi256, || sgr(Rgb(255, 0, 0), 38));
        assert_eq!(red, "38;5;196");
    }

    #[test]
    fn dim_text_is_faint() {
        assert_eq!(with_color_mode(ColorMode::TrueColor, || dim("x")), "\x1b[2mx\x1b[22m");
    }
}

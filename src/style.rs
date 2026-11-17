use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::output::Target;
use crate::text::Token;

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
#[non_exhaustive]
pub enum ColorMode {
    /// Plain text, no escape codes at all: piped output, `NO_COLOR`, or tests.
    None,
    /// The 256-colour palette, for terminals that don't advertise truecolor.
    Ansi256,
    /// Full 24-bit colour.
    TrueColor,
}

/// A colour mode as a byte for the atomics below; 0 is "not decided".
fn mode_code(mode: ColorMode) -> u8 {
    match mode {
        ColorMode::None => 1,
        ColorMode::Ansi256 => 2,
        ColorMode::TrueColor => 3,
    }
}

fn mode_from(code: u8) -> Option<ColorMode> {
    match code {
        1 => Some(ColorMode::None),
        2 => Some(ColorMode::Ansi256),
        3 => Some(ColorMode::TrueColor),
        _ => None,
    }
}

// The mode chosen with set_color_mode, which wins over anything worked out; 0 = none chosen.
static CHOSEN: AtomicU8 = AtomicU8::new(0);
// The mode worked out for stdout and for stderr, each decided the first time it's needed.
static DETECTED: [AtomicU8; 2] = [AtomicU8::new(0), AtomicU8::new(0)];

fn stream_slot(stream: Target) -> &'static AtomicU8 {
    match stream {
        Target::Stdout => &DETECTED[0],
        Target::Stderr => &DETECTED[1],
    }
}

/// The colour mode in use. Decided once, from `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`,
/// whether stdout (or the [`Target`](crate::Target)) is a terminal, `TERM`, `CLICOLOR` and
/// `COLORTERM`, unless [`set_color_mode`] has chosen one.
pub fn color_mode() -> ColorMode {
    color_mode_for(crate::output::target())
}

/// The colour mode for text written to `stream`, which may differ from [`color_mode`] when
/// one of stdout and stderr is a terminal and the other isn't. A mode chosen with
/// [`set_color_mode`] or [`with_color_mode`] holds for both.
pub(crate) fn color_mode_for(stream: Target) -> ColorMode {
    if let Some(mode) = SCOPED_MODE.get() {
        return mode;
    }
    if let Some(mode) = mode_from(CHOSEN.load(Ordering::Relaxed)) {
        return mode;
    }
    let slot = stream_slot(stream);
    if let Some(mode) = mode_from(slot.load(Ordering::Relaxed)) {
        return mode;
    }
    let (mode, _) = decide(&ColorEnv::read(), stream_is_terminal(stream), stream);
    if mode != ColorMode::None {
        crate::term::enable_escapes();
    }
    // threads deciding at once decide alike; whichever lands first is kept
    let _ = slot.compare_exchange(0, mode_code(mode), Ordering::Relaxed, Ordering::Relaxed);
    mode_from(slot.load(Ordering::Relaxed)).unwrap_or(mode)
}

/// The colour mode for stderr, where errors go whatever the [`Target`](crate::Target).
#[allow(dead_code)] // for the writers to stderr
pub(crate) fn stderr_color_mode() -> ColorMode {
    color_mode_for(Target::Stderr)
}

/// Run `f` in the colour mode for `stream`, so what it paints suits where it's written: an
/// error painted for stderr has no codes when stderr goes to a file, and has colour when only
/// stdout is piped. A mode already chosen for this thread is kept.
#[allow(dead_code)] // for the writers to stderr
pub(crate) fn for_stream<T>(stream: Target, f: impl FnOnce() -> T) -> T {
    match SCOPED_MODE.get() {
        Some(_) => f(),
        None => with_color_mode(color_mode_for(stream), f),
    }
}

/// `s`, already painted, made fit for `stream`: its escape codes dropped if that stream
/// has no colour.
#[allow(dead_code)] // for the writers to stderr
pub(crate) fn fit_for(stream: Target, s: &str) -> std::borrow::Cow<'_, str> {
    if color_mode_for(stream) == ColorMode::None && s.contains('\x1b') {
        crate::text::strip_ansi(s).into()
    } else {
        s.into()
    }
}

fn stream_is_terminal(stream: Target) -> bool {
    use std::io::IsTerminal;
    if let Some(terminal) = crate::output::pretended_terminal() {
        return terminal;
    }
    match stream {
        Target::Stdout => std::io::stdout().is_terminal(),
        Target::Stderr => std::io::stderr().is_terminal(),
    }
}

thread_local! {
    static SCOPED_MODE: std::cell::Cell<Option<ColorMode>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
thread_local! {
    static TEST_THEME: std::cell::Cell<Option<Theme>> = const { std::cell::Cell::new(None) };
}

/// Run `f` in a colour mode on this thread alone, leaving the rest of the program as it
/// was. Tests run side by side on threads of one process, so this is how a test that wants
/// real colour codes gets them without turning colour on, or off, for the tests beside it.
/// Anything drawn on other threads meanwhile uses the program's mode.
///
/// ```
/// use tinsel::{ColorMode, palette, paint, with_color_mode};
///
/// let lit = with_color_mode(ColorMode::TrueColor, || paint(palette::ROSE, "hi"));
/// assert!(lit.starts_with("\x1b[38;2;"));
/// assert_eq!(with_color_mode(ColorMode::None, || paint(palette::ROSE, "hi")), "hi");
/// ```
pub fn with_color_mode<T>(mode: ColorMode, f: impl FnOnce() -> T) -> T {
    let before = SCOPED_MODE.replace(Some(mode));
    // put back even if `f` panics, since tests catch panics and carry on
    struct Restore(Option<ColorMode>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPED_MODE.set(self.0);
        }
    }
    let _restore = Restore(before);
    f()
}

/// Run `f` without colour on this thread, unless [`with_color_mode`] already chose a mode.
pub(crate) fn plain_unless_chosen<T>(f: impl FnOnce() -> T) -> T {
    match SCOPED_MODE.get() {
        Some(_) => f(),
        None => with_color_mode(ColorMode::None, f),
    }
}

/// Run `f` in a theme on this thread alone.
#[cfg(test)]
pub(crate) fn with_theme<T>(theme: Theme, f: impl FnOnce() -> T) -> T {
    let before = TEST_THEME.replace(Some(theme));
    let out = f();
    TEST_THEME.set(before);
    out
}

/// Why the colour mode is what it is, for [`doctor`](crate::doctor).
pub(crate) fn color_why() -> String {
    if SCOPED_MODE.get().is_some() {
        return "chosen for this closure, with with_color_mode".into();
    }
    if CHOSEN.load(Ordering::Relaxed) != 0 {
        return "set in code".into();
    }
    let stream = crate::output::target();
    decide(&ColorEnv::read(), stream_is_terminal(stream), stream).1
}

/// Decide the colour mode again next time, unless it was chosen with [`set_color_mode`].
pub(crate) fn forget_detected() {
    for slot in &DETECTED {
        slot.store(0, Ordering::Relaxed);
    }
}

/// Choose the colour mode yourself, for a `--color` flag or to get plain text in tests.
///
/// ```
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// assert_eq!(tinsel::paint(tinsel::palette::ROSE, "hi"), "hi");
/// ```
pub fn set_color_mode(mode: ColorMode) {
    CHOSEN.store(mode_code(mode), Ordering::Relaxed);
}

/// The environment variables that have a say in the colour mode.
#[derive(Default)]
struct ColorEnv {
    no_color: Option<String>,
    force_color: Option<String>,
    clicolor_force: Option<String>,
    clicolor: Option<String>,
    term: Option<String>,
    colorterm: Option<String>,
}

impl ColorEnv {
    fn read() -> Self {
        let var = |name: &str| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        ColorEnv {
            no_color: var("NO_COLOR"),
            force_color: var("FORCE_COLOR"),
            clicolor_force: var("CLICOLOR_FORCE"),
            clicolor: var("CLICOLOR"),
            term: var("TERM"),
            colorterm: var("COLORTERM"),
        }
    }
}

/// The colour mode for a stream, and why. In order: `NO_COLOR` turns colour off;
/// `FORCE_COLOR` turns it off (`0`, `false`) or on, and `CLICOLOR_FORCE` on, even when the
/// stream isn't a terminal; otherwise a stream that isn't a terminal, `TERM=dumb` or
/// `CLICOLOR=0` means no colour. With colour on, the depth comes from `COLORTERM`, or from
/// `FORCE_COLOR`'s level (1 is 16 colours, 2 is 256, 3 is truecolor).
fn decide(env: &ColorEnv, terminal: bool, stream: Target) -> (ColorMode, String) {
    let set = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.is_empty());
    if set(&env.no_color) {
        return (ColorMode::None, "NO_COLOR is set".into());
    }
    if let Some(v) = env.force_color.as_deref() {
        let level = match v.trim().to_ascii_lowercase().as_str() {
            "0" | "false" => return (ColorMode::None, format!("FORCE_COLOR={v}")),
            // 16 colours would be level 1; 256 is the fewest tinsel draws with
            "1" | "2" => Some(ColorMode::Ansi256),
            "3" => Some(ColorMode::TrueColor),
            _ => None,
        };
        return match level {
            Some(mode) => (mode, format!("FORCE_COLOR={v}")),
            None => forced(env, format!("FORCE_COLOR={v}")),
        };
    }
    if let Some(v) = env.clicolor_force.as_deref().filter(|v| !v.is_empty() && *v != "0") {
        return forced(env, format!("CLICOLOR_FORCE={v}"));
    }
    if !terminal {
        let name = match stream {
            Target::Stdout => "stdout",
            Target::Stderr => "stderr",
        };
        return (ColorMode::None, format!("{name} isn't a terminal"));
    }
    if env.term.as_deref() == Some("dumb") {
        return (ColorMode::None, "TERM=dumb".into());
    }
    if env.clicolor.as_deref() == Some("0") {
        return (ColorMode::None, "CLICOLOR=0".into());
    }
    depth(env)
}

/// Colour turned on by a variable, as deep as `COLORTERM` allows.
fn forced(env: &ColorEnv, why: String) -> (ColorMode, String) {
    let (mode, depth_why) = depth(env);
    (mode, format!("{why}, and {depth_why}"))
}

/// How many colours the terminal has, from `COLORTERM`.
fn depth(env: &ColorEnv) -> (ColorMode, String) {
    match env.colorterm.as_deref() {
        Some(v) if v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit") => {
            (ColorMode::TrueColor, format!("COLORTERM={v}"))
        }
        Some(v) => (ColorMode::Ansi256, format!("COLORTERM={v} doesn't promise truecolor")),
        None => (
            ColorMode::Ansi256,
            "COLORTERM isn't set, so 256 colours to be safe".into(),
        ),
    }
}

/// Whether the terminal's background is dark or light. tinsel's palette is designed for dark
/// backgrounds; on light ones every colour is redrawn darker, keeping its hue, so pinks stay
/// pink and text stays readable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Theme {
    /// Light text on a dark background.
    Dark,
    /// Dark text on a light background.
    Light,
}

// The theme chosen with set_theme, which wins over the one worked out: 0 = none chosen,
// 1 = dark, 2 = light.
static THEME_CHOSEN: AtomicU8 = AtomicU8::new(0);
// The theme worked out, and how. Only one thread ever asks the terminal: any others that
// need the theme meanwhile wait for its answer rather than asking again over it.
static THEME_DETECTED: std::sync::OnceLock<(Theme, String)> = std::sync::OnceLock::new();

/// The theme in use. Decided once: from `TINSEL_THEME` (`light` or `dark`) if set, then the
/// `COLORFGBG` variable some terminals export, then by asking the terminal for its background
/// colour. Falls back to [`Theme::Dark`].
pub fn theme() -> Theme {
    #[cfg(test)]
    if let Some(theme) = TEST_THEME.get() {
        return theme;
    }
    match THEME_CHOSEN.load(Ordering::Relaxed) {
        1 => Theme::Dark,
        2 => Theme::Light,
        _ => detected_theme().0,
    }
}

fn detected_theme() -> &'static (Theme, String) {
    THEME_DETECTED.get_or_init(detect_theme)
}

/// Choose the theme yourself, for a `--theme` flag or to preview the other one.
pub fn set_theme(theme: Theme) {
    THEME_CHOSEN.store(if theme == Theme::Light { 2 } else { 1 }, Ordering::Relaxed);
}

/// Why the theme is what it is, for [`doctor`](crate::doctor).
pub(crate) fn theme_why() -> String {
    match THEME_CHOSEN.load(Ordering::Relaxed) {
        0 => detected_theme().1.clone(),
        _ => "set in code".into(),
    }
}

fn detect_theme() -> (Theme, String) {
    let (name, colorfgbg) = (std::env::var("TINSEL_THEME").ok(), std::env::var("COLORFGBG").ok());
    pick_theme_why(name.as_deref(), colorfgbg.as_deref(), crate::term::query_background)
}

/// The theme from `TINSEL_THEME`, then `COLORFGBG`, then the terminal's background colour.
#[cfg(test)]
fn pick_theme(name: Option<&str>, colorfgbg: Option<&str>, background: fn() -> Option<(u8, u8, u8)>) -> Theme {
    pick_theme_why(name, colorfgbg, background).0
}

/// [`pick_theme`], saying how it was decided.
fn pick_theme_why(
    name: Option<&str>,
    colorfgbg: Option<&str>,
    background: fn() -> Option<(u8, u8, u8)>,
) -> (Theme, String) {
    match name {
        Some(n @ "light") => return (Theme::Light, format!("TINSEL_THEME={n}")),
        Some(n @ "dark") => return (Theme::Dark, format!("TINSEL_THEME={n}")),
        _ => {}
    }
    // "fg;bg" in the 16-colour palette: 7 and 9-15 are light backgrounds
    if let Some(bg) = colorfgbg.and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok()) {
        let theme = if bg == 7 || bg > 8 { Theme::Light } else { Theme::Dark };
        return (theme, format!("COLORFGBG={}", colorfgbg.unwrap_or_default()));
    }
    if color_mode() == ColorMode::None {
        return (
            Theme::Dark,
            "the default: with colour off, the terminal wasn't asked".into(),
        );
    }
    match background() {
        Some((r, g, b)) if luminance(Rgb(r, g, b)) > 0.5 => (Theme::Light, "the terminal's background is light".into()),
        Some(_) => (Theme::Dark, "the terminal's background is dark".into()),
        None => (
            Theme::Dark,
            "the default: the terminal didn't say its background".into(),
        ),
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
#[non_exhaustive]
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

/// The accent in use, by name if it has one, for [`doctor`](crate::doctor).
pub(crate) fn accent_name() -> String {
    hue_name(accent_hue())
}

/// An accent's name, said as [`accent_name`] says it.
pub(crate) fn accent_label(accent: Accent) -> String {
    hue_name(accent.hue())
}

fn hue_name(hue: f32) -> String {
    let named = [Accent::Pink, Accent::Violet, Accent::Blue, Accent::Teal, Accent::Orange];
    let names = ["pink", "violet", "blue", "teal", "orange"];
    match named.iter().position(|a| (a.hue() - hue).abs() < 0.01) {
        Some(i) => names[i].into(),
        None => format!("a hue of {}°", hue.round()),
    }
}

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
        format!("{layer};5;{}", ansi256(Rgb(r, g, b)))
    }
}

/// The nearest colour in xterm's 256-colour palette: the closer of the best match in its
/// 6×6×6 cube (whose levels are 0, 95, 135, 175, 215 and 255, not evenly spaced) and the
/// best in its ramp of 24 greys (8 to 238 in steps of 10), which holds the dark greys the
/// cube can't.
pub(crate) fn ansi256(Rgb(r, g, b): Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |c: u8| match c {
        0..48 => 0,
        48..115 => 1,
        _ => (c - 35) / 40,
    };
    let distance = |(x, y, z): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(x, r) + d(y, g) + d(z, b)
    };
    let (qr, qg, qb) = (level(r), level(g), level(b));
    let cube = 16 + 36 * qr + 6 * qg + qb;
    let cube_rgb = (LEVELS[qr as usize], LEVELS[qg as usize], LEVELS[qb as usize]);
    let mean = (r as u16 + g as u16 + b as u16) / 3;
    let step = (mean.saturating_sub(3) / 10).min(23) as u8;
    let grey = 8 + 10 * step;
    if distance((grey, grey, grey)) < distance(cube_rgb) {
        232 + step
    } else {
        cube
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
    // how far [`wrap`](Line::wrap) indents the pieces after the first: a kv line's key column
    hang: usize,
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
        // a column of 10 for the keys, and always a space after a longer one
        let key = Line::new().push(&format!("{key:<9} "), palette::MUTED);
        let hang = key.width;
        Line {
            hang,
            ..key.push(value, color)
        }
    }

    /// How many columns the line takes up on screen.
    pub fn width(&self) -> usize {
        self.width
    }

    /// The line cut into pieces no wider than `cols`, its colours and links carried across
    /// each cut. It breaks at a space where it can and mid-word where it must (a long URL), and
    /// a [`kv`](Line::kv) line hangs the pieces after the first under its value.
    ///
    /// ```
    /// use tinsel::{Line, palette};
    ///
    /// let line = Line::kv("url", "https://example.com/a/rather/long/path", palette::TEXT);
    /// assert!(line.wrap(30).iter().all(|piece| piece.width() <= 30));
    /// ```
    pub fn wrap(&self, cols: usize) -> Vec<Line> {
        let cols = cols.max(1);
        if self.width <= cols {
            return vec![self.clone()];
        }
        let hang = if self.hang < cols { self.hang } else { 0 };
        let plain: Vec<(&str, usize)> = crate::text::tokens(&self.text)
            .filter_map(|t| match t {
                Token::Text(t, w) => Some((t, w)),
                Token::Escape(_) => None,
            })
            .collect();
        let ranges = wrap_ranges(&plain, cols, hang);

        // Walk the styled text, copying each visible cluster into the piece whose range holds
        // it and dropping the spaces a break swallowed. Every colour code seen so far is kept, so
        // a new piece can replay them and start in the state the last one stopped in; each piece
        // but the last is closed with a reset (and an open link ended) so its padding stays plain.
        let mut pieces = Vec::new();
        let mut piece = Line::new();
        let (mut colours, mut link) = (String::new(), None::<String>);
        let (mut k, mut v) = (0, 0);
        for token in crate::text::tokens(&self.text) {
            let (t, w) = match token {
                Token::Escape(code) => {
                    if code.starts_with("\x1b[") && code.ends_with('m') {
                        colours += code;
                    } else if code.starts_with("\x1b]8;") {
                        // a link's own end is `ESC ]8;;` with nothing after it
                        let ended = code.trim_end_matches(['\x07', '\\', '\x1b']).ends_with(";;");
                        link = (!ended).then(|| code.to_string());
                    }
                    piece.text += code;
                    continue;
                }
                Token::Text(t, w) => (t, w),
            };
            while k + 1 < ranges.len() && v >= ranges[k].1 {
                if link.is_some() {
                    piece.text += "\x1b]8;;\x1b\\";
                }
                if !colours.is_empty() {
                    piece.text += "\x1b[0m";
                }
                pieces.push(std::mem::take(&mut piece));
                piece = Line::new().space(hang);
                piece.text += &colours;
                piece.text += link.as_deref().unwrap_or("");
                k += 1;
            }
            if (ranges[k].0..ranges[k].1).contains(&v) {
                piece.text += t;
                piece.width += w;
            }
            v += 1;
        }
        pieces.push(piece);
        pieces
    }
}

/// Where [`Line::wrap`] cuts plain text, given as clusters and their widths: `[start, end)`
/// ranges of clusters, the first `cols` wide and the rest `cols - hang`. A cut goes at the
/// last space that fits (never one inside the first `hang` columns, which hold a kv line's
/// key), or mid-word when there isn't one, and the spaces either side of it belong to neither
/// piece.
fn wrap_ranges(plain: &[(&str, usize)], cols: usize, hang: usize) -> Vec<(usize, usize)> {
    let space = |i: usize| plain[i].0 == " ";
    let mut ranges = Vec::new();
    let (mut start, mut used, mut room) = (0, 0, cols);
    let mut i = 0;
    while i < plain.len() {
        let w = plain[i].1;
        // a character wider than the whole room still goes somewhere
        if used + w <= room || used == 0 {
            used += w;
            i += 1;
            continue;
        }
        let floor = if ranges.is_empty() { hang } else { 0 };
        let (mut col, mut last_space) = (0, None);
        for (j, &(t, w)) in plain.iter().enumerate().take(i).skip(start) {
            if t == " " && col >= floor && j > start {
                last_space = Some(j);
            }
            col += w;
        }
        // the character that didn't fit may be the space itself
        let end = if space(i) { i } else { last_space.unwrap_or(i) };
        let mut end_trim = end;
        while end_trim > start && space(end_trim - 1) {
            end_trim -= 1;
        }
        ranges.push((start, end_trim));
        start = end;
        while start < plain.len() && space(start) {
            start += 1;
        }
        (i, used, room) = (start, 0, cols - hang);
    }
    if start < plain.len() || ranges.is_empty() {
        ranges.push((start, plain.len()));
    }
    ranges
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

    fn env(vars: &[(&str, &str)]) -> ColorEnv {
        let get = |name: &str| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string());
        ColorEnv {
            no_color: get("NO_COLOR"),
            force_color: get("FORCE_COLOR"),
            clicolor_force: get("CLICOLOR_FORCE"),
            clicolor: get("CLICOLOR"),
            term: get("TERM"),
            colorterm: get("COLORTERM"),
        }
    }

    #[test]
    fn colour_depth_comes_from_the_terminal_and_the_environment() {
        let mode = |vars: &[(&str, &str)], terminal: bool| decide(&env(vars), terminal, Target::Stdout).0;
        assert_eq!(
            mode(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")], true),
            ColorMode::None
        );
        assert_eq!(mode(&[("NO_COLOR", "")], true), ColorMode::Ansi256);
        assert_eq!(mode(&[], false), ColorMode::None);
        assert_eq!(mode(&[("COLORTERM", "24bit")], true), ColorMode::TrueColor);
        assert_eq!(mode(&[("COLORTERM", "TrueColor")], true), ColorMode::TrueColor);
        assert_eq!(mode(&[], true), ColorMode::Ansi256);
        assert_eq!(
            crate::output::with_terminal(false, color_mode_for_test),
            ColorMode::None
        );
    }

    fn color_mode_for_test() -> ColorMode {
        decide(&ColorEnv::read(), stream_is_terminal(Target::Stdout), Target::Stdout).0
    }

    #[test]
    fn colour_can_be_forced_on_or_turned_off() {
        let mode = |vars: &[(&str, &str)], terminal: bool| decide(&env(vars), terminal, Target::Stdout).0;
        // forced on, even when piped
        assert_eq!(mode(&[("FORCE_COLOR", "1")], false), ColorMode::Ansi256);
        assert_eq!(mode(&[("FORCE_COLOR", "3")], false), ColorMode::TrueColor);
        assert_eq!(
            mode(&[("FORCE_COLOR", ""), ("COLORTERM", "truecolor")], false),
            ColorMode::TrueColor
        );
        assert_eq!(mode(&[("CLICOLOR_FORCE", "1")], false), ColorMode::Ansi256);
        assert_eq!(mode(&[("CLICOLOR_FORCE", "0")], false), ColorMode::None);
        assert_eq!(
            mode(&[("FORCE_COLOR", "1"), ("TERM", "dumb")], true),
            ColorMode::Ansi256
        );
        // turned off
        assert_eq!(
            mode(&[("FORCE_COLOR", "0"), ("CLICOLOR_FORCE", "1")], true),
            ColorMode::None
        );
        assert_eq!(mode(&[("FORCE_COLOR", "false")], true), ColorMode::None);
        assert_eq!(mode(&[("NO_COLOR", "1"), ("FORCE_COLOR", "1")], true), ColorMode::None);
        assert_eq!(mode(&[("TERM", "dumb")], true), ColorMode::None);
        assert_eq!(mode(&[("CLICOLOR", "0")], true), ColorMode::None);
        assert_eq!(mode(&[("CLICOLOR", "1")], true), ColorMode::Ansi256);
    }

    #[test]
    fn the_reason_matches_the_decision() {
        let why = |vars: &[(&str, &str)], terminal: bool| decide(&env(vars), terminal, Target::Stderr);
        assert_eq!(
            why(&[("COLORTERM", "TrueColor")], true),
            (ColorMode::TrueColor, "COLORTERM=TrueColor".into())
        );
        assert_eq!(why(&[], false), (ColorMode::None, "stderr isn't a terminal".into()));
        assert_eq!(
            why(&[("CLICOLOR_FORCE", "1")], false),
            (
                ColorMode::Ansi256,
                "CLICOLOR_FORCE=1, and COLORTERM isn't set, so 256 colours to be safe".into()
            )
        );
    }

    #[test]
    fn each_stream_gets_its_own_mode_unless_one_is_chosen() {
        with_color_mode(ColorMode::TrueColor, || {
            assert_eq!(color_mode_for(Target::Stderr), ColorMode::TrueColor);
            assert_eq!(fit_for(Target::Stderr, "\x1b[1mx\x1b[22m"), "\x1b[1mx\x1b[22m");
            assert_eq!(for_stream(Target::Stderr, || bold("x")), "\x1b[1mx\x1b[22m");
        });
        with_color_mode(ColorMode::None, || {
            assert_eq!(stderr_color_mode(), ColorMode::None);
            assert_eq!(fit_for(Target::Stderr, "\x1b[1mx\x1b[22m"), "x");
        });
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
    fn the_256_palette_is_matched_by_its_real_levels_and_greys() {
        // a near-black stays near black, from the grey ramp, not mid grey
        assert_eq!(ansi256(Rgb(38, 32, 36)), 235);
        assert_eq!(ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(ansi256(Rgb(128, 128, 128)), 244);
        // cube levels: 95 is level 1, 100 is nearer 95 than 135
        assert_eq!(ansi256(Rgb(100, 0, 0)), 16 + 36);
        assert_eq!(ansi256(Rgb(255, 0, 0)), 196);
        assert_eq!(ansi256(Rgb(0, 175, 0)), 16 + 6 * 3);
        for c in 0..=255u8 {
            assert!(ansi256(Rgb(c, c / 2, 255 - c)) >= 16);
        }
    }

    #[test]
    fn wrapping_a_line_keeps_characters_whole() {
        let pieces = with_color_mode(ColorMode::None, || {
            Line::new()
                .push("e\u{301}e\u{301}e\u{301} 日本語日本", palette::TEXT)
                .wrap(4)
        });
        let texts: Vec<String> = pieces.iter().map(|p| p.to_string()).collect();
        assert_eq!(texts, ["e\u{301}e\u{301}e\u{301}", "日本", "語日", "本"]);
        assert!(pieces.iter().all(|p| p.width() <= 4));
    }

    #[test]
    fn dim_text_is_faint() {
        assert_eq!(with_color_mode(ColorMode::TrueColor, || dim("x")), "\x1b[2mx\x1b[22m");
    }
}

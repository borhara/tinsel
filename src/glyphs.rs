//! Which characters tinsel draws with. Unicode by default: lines, blocks, dots and
//! spinners. ASCII where the terminal's font can't draw those, like the Linux console's, or
//! where a terminal draws Unicode's ambiguous characters two columns wide, as one set up for
//! Chinese, Japanese or Korean can: the lines themselves are ambiguous, so drawn wide
//! they'd break every layout. Each symbol becomes one ASCII character, so nothing moves.
//!
//! The switch happens as text leaves for the terminal, so every widget gets it for free,
//! and so does an app's own text: its curly quotes, dashes, arrows and ellipses go as ASCII
//! too, as a font that lacks the lines likely lacks those, and drawn wide they'd move
//! everything after them. Letters and wide characters are left as they are.

use std::borrow::Cow;
use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// The characters tinsel draws its lines, blocks, dots and spinners with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Glyphs {
    /// Unicode's: `╭─╮ ● ▁▄█ ✧`.
    Unicode,
    /// Plain ASCII, for a terminal whose font hasn't those: `+-+ * .:# +`.
    Ascii,
}

thread_local! {
    static SCOPED_GLYPHS: Cell<Option<Glyphs>> = const { Cell::new(None) };
    static SCOPED_AMBIGUOUS: Cell<Option<bool>> = const { Cell::new(None) };
}

// set in code: 0 = not, then 1 + the choice
static GLYPHS_CHOSEN: AtomicU8 = AtomicU8::new(0);
static AMBIGUOUS_CHOSEN: AtomicU8 = AtomicU8::new(0);
static GLYPHS_FOUND: OnceLock<(Glyphs, String)> = OnceLock::new();
static AMBIGUOUS_FOUND: OnceLock<(bool, String)> = OnceLock::new();

/// The characters tinsel draws with. Decided once: from `TINSEL_GLYPHS` (`unicode` or
/// `ascii`) if set, and ASCII on the Linux console (`TERM=linux`), whose font has few
/// symbols. Unicode otherwise. Wide ambiguous characters (see [`ambiguous_wide`]) mean ASCII too.
pub fn glyphs() -> Glyphs {
    if let Some(g) = SCOPED_GLYPHS.get() {
        return g;
    }
    match GLYPHS_CHOSEN.load(Ordering::Relaxed) {
        1 => Glyphs::Unicode,
        2 => Glyphs::Ascii,
        _ => GLYPHS_FOUND.get_or_init(|| pick_glyphs(&env)).0,
    }
}

/// Choose the characters yourself, for a `--ascii` flag say.
pub fn set_glyphs(g: Glyphs) {
    GLYPHS_CHOSEN.store(if g == Glyphs::Ascii { 2 } else { 1 }, Ordering::Relaxed);
}

/// Runs `f` drawing with `g`, on this thread only, for tests.
pub fn with_glyphs<T>(g: Glyphs, f: impl FnOnce() -> T) -> T {
    let before = SCOPED_GLYPHS.replace(Some(g));
    let out = f();
    SCOPED_GLYPHS.set(before);
    out
}

/// Whether the terminal draws Unicode's ambiguous characters two columns wide, as one set up
/// for Chinese, Japanese or Korean can (iTerm2's "Ambiguous characters are double-width",
/// GNOME Terminal's "Ambiguous-width characters: Wide"). Then tinsel measures text that has
/// them as wide, and draws its own symbols in ASCII. From `TINSEL_AMBIGUOUS` (`wide` or
/// `narrow`); narrow otherwise, as most terminals are.
pub fn ambiguous_wide() -> bool {
    if let Some(w) = SCOPED_AMBIGUOUS.get() {
        return w;
    }
    match AMBIGUOUS_CHOSEN.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => AMBIGUOUS_FOUND.get_or_init(|| pick_ambiguous(&env)).0,
    }
}

/// Say yourself whether the terminal draws ambiguous characters wide.
pub fn set_ambiguous_wide(wide: bool) {
    AMBIGUOUS_CHOSEN.store(if wide { 2 } else { 1 }, Ordering::Relaxed);
}

/// Runs `f` as if the terminal drew ambiguous characters wide or not, on this thread only,
/// for tests.
pub fn with_ambiguous_wide<T>(wide: bool, f: impl FnOnce() -> T) -> T {
    let before = SCOPED_AMBIGUOUS.replace(Some(wide));
    let out = f();
    SCOPED_AMBIGUOUS.set(before);
    out
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn pick_glyphs(var: &dyn Fn(&str) -> Option<String>) -> (Glyphs, String) {
    match var("TINSEL_GLYPHS").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("ascii") => return (Glyphs::Ascii, "TINSEL_GLYPHS=ascii".into()),
        Some("unicode") => return (Glyphs::Unicode, "TINSEL_GLYPHS=unicode".into()),
        _ => {}
    }
    if var("TERM").as_deref() == Some("linux") {
        return (
            Glyphs::Ascii,
            "TERM=linux: the Linux console's font has few symbols".into(),
        );
    }
    (Glyphs::Unicode, "the default".into())
}

fn pick_ambiguous(var: &dyn Fn(&str) -> Option<String>) -> (bool, String) {
    match var("TINSEL_AMBIGUOUS")
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("wide") => (true, "TINSEL_AMBIGUOUS=wide".into()),
        Some("narrow") => (false, "TINSEL_AMBIGUOUS=narrow".into()),
        _ => (false, "the default, as most terminals draw them".into()),
    }
}

/// Why the characters are what they are, for [`doctor`](crate::doctor).
pub(crate) fn glyphs_why() -> String {
    if SCOPED_GLYPHS.get().is_some() {
        return "chosen for this closure, with with_glyphs".into();
    }
    match GLYPHS_CHOSEN.load(Ordering::Relaxed) {
        0 if ambiguous_wide() && GLYPHS_FOUND.get_or_init(|| pick_glyphs(&env)).0 == Glyphs::Unicode => {
            "ASCII for tinsel's own symbols, as ambiguous characters are wide".into()
        }
        0 => GLYPHS_FOUND.get_or_init(|| pick_glyphs(&env)).1.clone(),
        _ => "set in code".into(),
    }
}

/// Why ambiguous characters are counted as they are, for [`doctor`](crate::doctor).
pub(crate) fn ambiguous_why() -> String {
    if SCOPED_AMBIGUOUS.get().is_some() {
        return "chosen for this closure, with with_ambiguous_wide".into();
    }
    match AMBIGUOUS_CHOSEN.load(Ordering::Relaxed) {
        0 => AMBIGUOUS_FOUND.get_or_init(|| pick_ambiguous(&env)).1.clone(),
        _ => "set in code".into(),
    }
}

/// Whether tinsel's symbols go out as ASCII: asked for, or ambiguous characters drawn wide.
pub(crate) fn ascii() -> bool {
    glyphs() == Glyphs::Ascii || ambiguous_wide()
}

/// The one ASCII character a symbol tinsel draws becomes, if it's one of them.
pub(crate) fn ascii_for(c: char) -> Option<char> {
    Some(match c {
        '─' | '━' | '┄' | '┈' | '╴' | '╶' | '–' => '-',
        '│' | '┃' | '╵' | '╷' | '▏' | '▎' | '▌' | '▐' => '|',
        '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╰' | '╯' => '+',
        '╱' => '/',
        // bars and sparklines, from low to high
        '▁' | '▂' | '░' => '.',
        '▃' | '▄' | '▒' => ':',
        '▅' | '▆' => '=',
        '▇' | '█' | '▓' => '#',
        '●' | '✶' | '✦' | '⋆' | '•' | '❤' => '*',
        '○' | '◌' | '◦' => 'o',
        '◉' => '@',
        '✧' => '+',
        '∙' | '…' => '.',
        '✕' | '×' => 'x',
        '▲' | '↑' | '⇞' | '⇧' | '⌃' => '^',
        '▼' | '↓' | '⇟' => 'v',
        '←' | '‹' | '⌫' | '⏎' => '<',
        '→' | '›' | '⇥' | '⌦' => '>',
        '↖' | '↘' => '\\',
        '⌥' => '~',
        '⎋' => 'e',
        '␣' => '_',
        '⊘' => '0',
        '“' | '”' => '"',
        // the arc spinner and the braille one go round as | / - \
        '◜' | '◞' | '⣽' | '⣟' => '/',
        '◝' | '◟' | '⢿' | '⣷' => '\\',
        '◠' | '◡' | '⣻' | '⣯' => '-',
        '⣾' | '⡿' => '|',
        _ => return None,
    })
}

/// `s` as it goes to the terminal: as it is, or with tinsel's symbols in ASCII.
pub(crate) fn for_terminal(s: &str) -> Cow<'_, str> {
    if !ascii() || s.is_ascii() || !s.chars().any(|c| ascii_for(c).is_some()) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(s.chars().map(|c| ascii_for(c).unwrap_or(c)).collect())
}

/// A stream that sends text through [`for_terminal`] on its way.
pub(crate) struct Glyphed<W: std::io::Write>(pub W);

impl<W: std::io::Write> std::io::Write for Glyphed<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(s) if ascii() => {
                self.0.write_all(for_terminal(s).as_bytes())?;
                Ok(buf.len())
            }
            // a character cut in two between writes can't be read, so goes as it is
            _ => self.0.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn the_environment_decides_unless_code_does() {
        assert_eq!(pick_glyphs(&vars(&[])).0, Glyphs::Unicode);
        assert_eq!(pick_glyphs(&vars(&[("TERM", "linux")])).0, Glyphs::Ascii);
        assert_eq!(
            pick_glyphs(&vars(&[("TERM", "linux"), ("TINSEL_GLYPHS", "unicode")])).0,
            Glyphs::Unicode
        );
        assert_eq!(pick_glyphs(&vars(&[("TINSEL_GLYPHS", "ASCII")])).0, Glyphs::Ascii);
        assert!(!pick_ambiguous(&vars(&[])).0);
        assert!(pick_ambiguous(&vars(&[("TINSEL_AMBIGUOUS", "wide")])).0);
    }

    #[test]
    fn every_symbol_becomes_one_ascii_character() {
        let drawn = "╭─╮│●▁▄█✧→⣾";
        let ascii = with_glyphs(Glyphs::Ascii, || for_terminal(drawn).into_owned());
        assert_eq!(ascii, "+-+|*.:#+>|");
        assert_eq!(crate::text::width(&ascii), crate::text::width(drawn), "nothing moves");
        assert_eq!(with_glyphs(Glyphs::Unicode, || for_terminal(drawn).into_owned()), drawn);
        // letters, wide characters and escapes are left alone; symbols are swapped wherever they are
        assert_eq!(
            with_glyphs(Glyphs::Ascii, || for_terminal("“Résumé” – 5×3 → done…").into_owned()),
            "\"Résumé\" - 5x3 > done."
        );
        assert_eq!(
            with_glyphs(Glyphs::Ascii, || for_terminal("é 日本 \x1b[1mbold\x1b[0m").into_owned()),
            "é 日本 \x1b[1mbold\x1b[0m"
        );
    }

    #[test]
    fn the_braille_spinner_still_goes_round() {
        let frames: String = "⣾⣽⣻⢿⡿⣟⣯⣷".chars().map(|c| ascii_for(c).unwrap()).collect();
        assert_eq!(frames, "|/-\\|/-\\");
    }

    #[test]
    fn wide_ambiguous_characters_mean_ascii_symbols() {
        with_glyphs(Glyphs::Unicode, || {
            assert!(!with_ambiguous_wide(false, ascii));
            assert!(with_ambiguous_wide(true, ascii));
            assert_eq!(with_ambiguous_wide(true, || for_terminal("──●").into_owned()), "--*");
        });
    }

    #[test]
    fn a_stream_sends_text_through_on_its_way() {
        use std::io::Write;
        let mut out = Glyphed(Vec::new());
        with_glyphs(Glyphs::Ascii, || write!(out, "╭─╮ hi").unwrap());
        assert_eq!(String::from_utf8(out.0).unwrap(), "+-+ hi");
    }
}

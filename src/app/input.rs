//! Turning the bytes a terminal sends into keys and mouse events.

use std::time::{Duration, Instant};

/// A key someone pressed.
///
/// Keys pressed on their own are their own variants, so `Key::Up`, `Key::Char('x')` and
/// `Key::Ctrl('r')` match as you'd expect. A key held with modifiers comes as one of the
/// others:
///
/// - Alt (Option on a Mac, where the terminal sends it as Meta) and a character is
///   [`Key::Alt`]: `Key::Alt('b')`.
/// - Any other key held with Shift, Alt or Ctrl is [`Key::Mod`], the modifiers and the key:
///   Ctrl-← is `Key::Mod(Mods::CTRL, NamedKey::Left)`, Shift-Alt-↑ is
///   `Key::Mod(Mods::SHIFT | Mods::ALT, NamedKey::Up)`. Shift-Tab stays [`Key::BackTab`].
///
/// [`Key::mods`] and [`Key::unmodified`] take a key apart, for an app that treats Ctrl-→
/// like → or wants to know whether Shift was down whatever the key.
///
/// ```
/// use tinsel::app::{Key, Mods, NamedKey};
///
/// let key = Key::Mod(Mods::CTRL, NamedKey::Left);
/// assert!(matches!(key, Key::Mod(m, NamedKey::Left) if m.ctrl()));
/// assert_eq!(key.unmodified(), Key::Left);
/// assert_eq!(Key::modified(NamedKey::Up, Mods::NONE), Key::Up);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Key {
    /// A printable character, space included.
    Char(char),
    /// Ctrl and a letter, as the lowercase letter. Ctrl-C is `Ctrl('c')`.
    Ctrl(char),
    /// Alt (or Meta, or Option) and a character, as typed: Alt-Shift-x is `Alt('X')`.
    Alt(char),
    /// Return.
    Enter,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Escape, on its own.
    Esc,
    /// ↑
    Up,
    /// ↓
    Down,
    /// ←
    Left,
    /// →
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// A function key, F1 to F24.
    F(u8),
    /// A key that isn't a character, held with modifiers (never [`Mods::NONE`]).
    Mod(Mods, NamedKey),
}

impl Key {
    /// `key` held with `mods`, as the terminal would send it: the plain variant with no
    /// modifiers, [`Key::BackTab`] for Shift-Tab, and [`Key::Mod`] otherwise.
    pub fn modified(key: NamedKey, mods: Mods) -> Key {
        if mods.is_empty() {
            return key.into();
        }
        if mods == Mods::SHIFT && key == NamedKey::Tab {
            return Key::BackTab;
        }
        Key::Mod(mods, key)
    }

    /// The modifiers held with this key: Ctrl for [`Key::Ctrl`], Alt for [`Key::Alt`],
    /// Shift for [`Key::BackTab`], none for the rest.
    pub fn mods(self) -> Mods {
        match self {
            Key::Ctrl(_) => Mods::CTRL,
            Key::Alt(_) => Mods::ALT,
            Key::BackTab => Mods::SHIFT,
            Key::Mod(mods, _) => mods,
            _ => Mods::NONE,
        }
    }

    /// The key with its modifiers let go: Ctrl-← is ←, Alt-b and Ctrl-B are `Char('b')`,
    /// Shift-Tab is Tab.
    #[must_use]
    pub fn unmodified(self) -> Key {
        match self {
            Key::Ctrl(c) | Key::Alt(c) => Key::Char(c),
            Key::BackTab => Key::Tab,
            Key::Mod(_, key) => key.into(),
            key => key,
        }
    }
}

/// Keys that aren't characters, for [`Key::Mod`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NamedKey {
    /// Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// ↑
    Up,
    /// ↓
    Down,
    /// ←
    Left,
    /// →
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// A function key, F1 to F24.
    F(u8),
}

impl From<NamedKey> for Key {
    fn from(key: NamedKey) -> Key {
        match key {
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Up => Key::Up,
            NamedKey::Down => Key::Down,
            NamedKey::Left => Key::Left,
            NamedKey::Right => Key::Right,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::F(n) => Key::F(n),
        }
    }
}

/// Which of Shift, Alt and Ctrl were held, for [`Key::Mod`] and [`Mouse::mods`]. Combine
/// them with `|`; the constants work in patterns too.
///
/// ```
/// use tinsel::app::Mods;
///
/// let held = Mods::CTRL | Mods::SHIFT;
/// assert!(held.ctrl() && held.shift() && !held.alt());
/// assert!(held.contains(Mods::CTRL));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    /// Nothing held.
    pub const NONE: Mods = Mods(0);
    /// Shift.
    pub const SHIFT: Mods = Mods(1);
    /// Alt, Meta or Option: terminals don't tell them apart.
    pub const ALT: Mods = Mods(2);
    /// Ctrl.
    pub const CTRL: Mods = Mods(4);

    /// Whether Shift was held.
    pub fn shift(self) -> bool {
        self.contains(Mods::SHIFT)
    }

    /// Whether Alt was held.
    pub fn alt(self) -> bool {
        self.contains(Mods::ALT)
    }

    /// Whether Ctrl was held.
    pub fn ctrl(self) -> bool {
        self.contains(Mods::CTRL)
    }

    /// Whether everything in `other` was held.
    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether nothing was held.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// From the number xterm puts after a `;` in a key's escape sequence: one more than
    /// a mask of Shift 1, Alt 2, Ctrl 4 and Meta 8 (taken as Alt).
    fn from_xterm(param: &str) -> Mods {
        let bits = param.parse::<u8>().unwrap_or(1).saturating_sub(1);
        Mods((bits & 7) | if bits & 8 != 0 { 2 } else { 0 })
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;

    fn bitor(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
}

impl std::ops::BitOrAssign for Mods {
    fn bitor_assign(&mut self, other: Mods) {
        self.0 |= other.0;
    }
}

/// What the mouse did. Only reported when [`App::mouse`](super::App::mouse) says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MouseKind {
    /// A left click.
    Click,
    /// A middle click (the wheel pressed).
    MiddleClick,
    /// A right click.
    RightClick,
    /// A button let go.
    Release,
    /// The mouse moved with a button held down.
    Drag,
    /// The wheel, rolled up.
    ScrollUp,
    /// The wheel, rolled down.
    ScrollDown,
}

/// A mouse event, at a cell counted from 0 at the top left. Check which panel it landed in
/// with [`Rect::contains`](super::Rect::contains).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Mouse {
    /// What happened.
    pub kind: MouseKind,
    /// The column.
    pub x: usize,
    /// The row.
    pub y: usize,
    /// Which of Shift, Alt and Ctrl were held, where the terminal says (many keep
    /// Shift-click for selecting text themselves).
    pub mods: Mods,
}

impl Mouse {
    /// A mouse event of this kind at column `x` and row `y`, with nothing held, as a test
    /// might send one.
    pub fn new(kind: MouseKind, x: usize, y: usize) -> Self {
        Mouse {
            kind,
            x,
            y,
            mods: Mods::NONE,
        }
    }
}

/// Bytes as they're read, handed back as events once they're whole. Unlike calling [`parse`]
/// again on everything, a paste is searched for its end only once, however many pieces it
/// arrives in.
pub(crate) struct Decoder {
    pending: Vec<u8>,
    /// How much of `pending` has already been turned into events.
    used: usize,
    /// Where to carry on looking for the end of a paste.
    searched: usize,
    /// When bytes last came.
    heard: Instant,
    /// How long a paste can go quiet before it's taken as ended.
    paste_wait: Duration,
}

/// The most a paste holds before it's handed over unfinished, so a terminal that never ends
/// one can't fill the memory.
const MAX_PASTE: usize = 4 << 20;

impl Decoder {
    pub(crate) fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            used: 0,
            searched: 0,
            heard: Instant::now(),
            paste_wait: Duration::from_millis(1500),
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        if self.used > 0 {
            self.pending.drain(..self.used);
            self.searched = self.searched.saturating_sub(self.used);
            self.used = 0;
        }
        if !bytes.is_empty() {
            self.pending.extend_from_slice(bytes);
            self.heard = Instant::now();
        }
    }

    /// The next event, as [`parse`] gives it: `None` while what's left could be the start of
    /// something longer, `Some(None)` for bytes let go. `done` says nothing more is coming
    /// for now.
    pub(crate) fn next(&mut self, done: bool) -> Option<Option<super::Event>> {
        let rest = &self.pending[self.used..];
        let (event, used) = if rest.starts_with(b"\x1b[") && rest[2..].starts_with(PASTE_START) {
            self.paste(done)?
        } else {
            parse(rest, done)?
        };
        self.used += used;
        if self.used == self.pending.len() {
            self.pending.clear();
            self.used = 0;
        }
        Some(event)
    }

    /// [`paste`], looking only at what came since last time. A paste that never ends (the
    /// terminal lost its end, or it wasn't really one) is taken as it stands once it's gone
    /// quiet or grown too big, and what follows is read as keys again, so Ctrl-C still works.
    fn paste(&mut self, done: bool) -> Option<(Option<super::Event>, usize)> {
        let start = self.used + 2 + PASTE_START.len();
        let from = self.searched.max(start);
        let Some(at) = self.pending[from..]
            .windows(PASTE_END.len())
            .position(|w| w == PASTE_END)
        else {
            // the end may be split between reads, so its first bytes are looked at again
            self.searched = self.pending.len().saturating_sub(PASTE_END.len() - 1).max(start);
            let stalled = done && self.heard.elapsed() >= self.paste_wait;
            if !stalled && self.pending.len() - start < MAX_PASTE {
                return None;
            }
            self.searched = 0;
            let text = pasted(&self.pending[start..]);
            return Some((Some(super::Event::Paste(text)), self.pending.len() - self.used));
        };
        self.searched = 0;
        let end = from + at;
        let text = pasted(&self.pending[start..end]);
        Some((Some(super::Event::Paste(text)), end + PASTE_END.len() - self.used))
    }
}

/// One input event read from the front of `bytes`, and how many bytes it used.
///
/// `None` means the bytes so far could be the start of something longer (an escape sequence,
/// or part of a UTF-8 character). If nothing more arrives, call again with `done` set and
/// whatever's there is taken as it stands: a lone Esc is a press of Esc.
/// `Some((None, n))` means `n` bytes were something to ignore.
pub(crate) fn parse(bytes: &[u8], done: bool) -> Option<(Option<super::Event>, usize)> {
    use super::Event;
    let key = |k: Key, n: usize| Some((Some(Event::Key(k)), n));
    let first = *bytes.first()?;
    match first {
        b'\r' | b'\n' => key(Key::Enter, 1),
        b'\t' => key(Key::Tab, 1),
        127 | 8 => key(Key::Backspace, 1),
        27 => match bytes.get(1) {
            None if done => key(Key::Esc, 1),
            None => None,
            Some(b'[') => csi(bytes, done),
            Some(b'O') => match bytes.get(2) {
                None if done => key(Key::Esc, 1),
                None => None,
                Some(&b) => match ss3(b) {
                    Some(k) => key(k, 3),
                    None => Some((None, 3)),
                },
            },
            Some(_) => alt(bytes, done),
        },
        1..=26 => key(Key::Ctrl((b'a' + first - 1) as char), 1),
        b if b < 0x20 => Some((None, 1)),
        b if b < 0x80 => key(Key::Char(b as char), 1),
        b => {
            let len = match b {
                0xF0.. => 4,
                0xE0.. => 3,
                0xC0.. => 2,
                _ => return Some((None, 1)),
            };
            if bytes.len() < len {
                return if done { Some((None, bytes.len())) } else { None };
            }
            match std::str::from_utf8(&bytes[..len]).ok().and_then(|s| s.chars().next()) {
                Some(c) => key(Key::Char(c), len),
                None => Some((None, 1)),
            }
        }
    }
}

/// Esc then a key, which is how terminals send that key pressed with Alt: Alt-x is `ESC x`.
/// Esc then something that isn't a key (another Esc, a control character) is Esc on its
/// own, and the rest is read after.
fn alt(bytes: &[u8], done: bool) -> Option<(Option<super::Event>, usize)> {
    use super::Event;
    let alone = Some((Some(Event::Key(Key::Esc)), 1));
    let with = |k: NamedKey| Some((Some(Event::Key(Key::Mod(Mods::ALT, k))), 2));
    match bytes[1] {
        b'\r' | b'\n' => with(NamedKey::Enter),
        b'\t' => with(NamedKey::Tab),
        127 | 8 => with(NamedKey::Backspace),
        b if b < 0x20 => alone,
        _ => match parse(&bytes[1..], done)? {
            (Some(Event::Key(Key::Char(c))), n) => Some((Some(Event::Key(Key::Alt(c))), n + 1)),
            // broken UTF-8 after an Esc: the Esc, then the rest let go as usual
            _ => alone,
        },
    }
}

/// `ESC [ params final`: arrows, Home/End and friends, function keys, and SGR mouse reports.
fn csi(bytes: &[u8], done: bool) -> Option<(Option<super::Event>, usize)> {
    use super::Event;
    if bytes[2..].starts_with(PASTE_START) {
        return paste(bytes);
    }
    // older terminals ignore SGR mouse mode and send `ESC [ M` and three raw bytes instead,
    // which mustn't be read as keys (a click in column 81 would be a `q`)
    if bytes.get(2) == Some(&b'M') {
        return match bytes.get(3..6) {
            Some(&[b, x, y]) => {
                let params = format!(
                    "{};{};{}",
                    b.saturating_sub(32),
                    x.saturating_sub(32),
                    y.saturating_sub(32)
                );
                // in this encoding a release is button 3, so anything else is a press
                Some((mouse(&params, true), 6))
            }
            _ if done => Some((None, bytes.len())),
            _ => None,
        };
    }
    // parameters, then intermediates, then the final byte; anything else means this wasn't a
    // sequence after all
    let mut end = 2;
    while bytes.get(end).is_some_and(|b| (0x30..=0x3F).contains(b)) {
        end += 1;
    }
    while bytes.get(end).is_some_and(|b| (0x20..=0x2F).contains(b)) {
        end += 1;
    }
    match bytes.get(end) {
        Some(0x40..=0x7E) => {}
        // a pause halfway through: give up on it as Esc, and read the rest as typed
        None if done => return Some((Some(Event::Key(Key::Esc)), 1)),
        None => return None,
        // broken off by something that can't be in one (another Esc, say): an Esc, and the
        // rest read again from the byte that broke it
        Some(_) => return Some((Some(Event::Key(Key::Esc)), end)),
    }
    let params = std::str::from_utf8(&bytes[2..end]).unwrap_or("");
    let n = end + 1;
    // modifiers, if any, come after a `;`: `ESC [ 1 ; 5 D` is Ctrl-←
    let mut parts = params.split(';');
    let code = parts.next().unwrap_or("");
    let mods = parts.next().map_or(Mods::NONE, Mods::from_xterm);
    let key = match (bytes[end], params) {
        (b'M' | b'm', p) if p.starts_with('<') => return Some((mouse(&p[1..], bytes[end] == b'M'), n)),
        (b'I', "") => return Some((Some(Event::Focus(true)), n)),
        (b'O', "") => return Some((Some(Event::Focus(false)), n)),
        (b'Z', _) => return Some((Some(Event::Key(Key::BackTab)), n)),
        (b'A', _) => NamedKey::Up,
        (b'B', _) => NamedKey::Down,
        (b'C', _) => NamedKey::Right,
        (b'D', _) => NamedKey::Left,
        (b'H', _) => NamedKey::Home,
        (b'F', _) => NamedKey::End,
        // F1 to F4 with modifiers: `ESC [ 1 ; 2 P` is Shift-F1
        (b @ b'P'..=b'S', _) if !mods.is_empty() => NamedKey::F(b - b'P' + 1),
        (b'~', _) => match code {
            "1" | "7" => NamedKey::Home,
            "4" | "8" => NamedKey::End,
            "3" => NamedKey::Delete,
            "5" => NamedKey::PageUp,
            "6" => NamedKey::PageDown,
            code => match code.parse::<u8>() {
                Ok(c @ 11..=15) => NamedKey::F(c - 10),
                Ok(c @ 17..=21) => NamedKey::F(c - 11),
                Ok(c @ 23..=24) => NamedKey::F(c - 12),
                _ => return Some((None, n)),
            },
        },
        _ => return Some((None, n)),
    };
    Some((Some(Event::Key(Key::modified(key, mods))), n))
}

/// What a terminal sends around pasted text, after `ESC [`, once bracketed paste is on.
const PASTE_START: &[u8] = b"200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// `ESC [ 200~ text ESC [ 201~`: pasted text, all of it, with its line breaks as `\n`. It's
/// waited for even through a pause, since a big paste over a slow link arrives in pieces, and
/// read as keys its newlines would be Enter.
fn paste(bytes: &[u8]) -> Option<(Option<super::Event>, usize)> {
    let start = 2 + PASTE_START.len();
    let len = bytes[start..].windows(PASTE_END.len()).position(|w| w == PASTE_END)?;
    let text = pasted(&bytes[start..start + len]);
    Some((Some(super::Event::Paste(text)), start + len + PASTE_END.len()))
}

/// Pasted bytes as text, with their line breaks as `\n`.
fn pasted(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn ss3(b: u8) -> Option<Key> {
    Some(match b {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'P'..=b'S' => Key::F(b - b'P' + 1),
        _ => return None,
    })
}

/// `button;x;y` from an SGR mouse report (1-based). The button's number carries the
/// modifiers held (Shift 4, Alt 8, Ctrl 16) and whether the mouse moved (32).
fn mouse(params: &str, pressed: bool) -> Option<super::Event> {
    let mut it = params.split(';').map(|p| p.parse::<usize>().ok());
    let (button, x, y) = (it.next()??, it.next()??, it.next()??);
    let mut mods = Mods::NONE;
    for (bit, m) in [(4, Mods::SHIFT), (8, Mods::ALT), (16, Mods::CTRL)] {
        if button & bit != 0 {
            mods |= m;
        }
    }
    let kind = match button & !(4 | 8 | 16) {
        64 => MouseKind::ScrollUp,
        65 => MouseKind::ScrollDown,
        // moving with no button held, which only some modes report
        35 => return None,
        32..=34 => MouseKind::Drag,
        _ if !pressed => MouseKind::Release,
        0 => MouseKind::Click,
        1 => MouseKind::MiddleClick,
        2 => MouseKind::RightClick,
        3 => MouseKind::Release,
        _ => return None,
    };
    Some(super::Event::Mouse(Mouse {
        kind,
        x: x.saturating_sub(1),
        y: y.saturating_sub(1),
        mods,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::Event;
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        let mut rest = bytes;
        while let Some((ev, n)) = parse(rest, true) {
            out.extend(ev);
            rest = &rest[n..];
        }
        out
    }

    #[test]
    fn reads_plain_keys_and_utf8() {
        assert_eq!(
            keys("aé ✦\r\x03\x7f".as_bytes()),
            [
                Event::Key(Key::Char('a')),
                Event::Key(Key::Char('é')),
                Event::Key(Key::Char(' ')),
                Event::Key(Key::Char('✦')),
                Event::Key(Key::Enter),
                Event::Key(Key::Ctrl('c')),
                Event::Key(Key::Backspace),
            ]
        );
    }

    #[test]
    fn reads_escape_sequences() {
        assert_eq!(
            keys(b"\x1b[A\x1bOB\x1b[5~\x1b[1;5C\x1b[Z\x1b[15~\x1bOP"),
            [
                Event::Key(Key::Up),
                Event::Key(Key::Down),
                Event::Key(Key::PageUp),
                Event::Key(Key::Mod(Mods::CTRL, NamedKey::Right)),
                Event::Key(Key::BackTab),
                Event::Key(Key::F(5)),
                Event::Key(Key::F(1)),
            ]
        );
    }

    #[test]
    fn reads_every_named_key() {
        let k = Event::Key;
        let cases: &[(&[u8], &[Event])] = &[
            (b"\t\x08\n", &[k(Key::Tab), k(Key::Backspace), k(Key::Enter)]),
            (
                b"\x1b[B\x1b[C\x1b[D\x1b[H\x1b[F",
                &[k(Key::Down), k(Key::Right), k(Key::Left), k(Key::Home), k(Key::End)],
            ),
            (
                b"\x1bOA\x1bOC\x1bOD\x1bOH\x1bOF\x1bOS",
                &[
                    k(Key::Up),
                    k(Key::Right),
                    k(Key::Left),
                    k(Key::Home),
                    k(Key::End),
                    k(Key::F(4)),
                ],
            ),
            (
                b"\x1b[1~\x1b[7~\x1b[4~\x1b[8~\x1b[3~\x1b[6~",
                &[
                    k(Key::Home),
                    k(Key::Home),
                    k(Key::End),
                    k(Key::End),
                    k(Key::Delete),
                    k(Key::PageDown),
                ],
            ),
            (
                b"\x1b[11~\x1b[17~\x1b[21~\x1b[23~\x1b[24~",
                &[k(Key::F(1)), k(Key::F(6)), k(Key::F(10)), k(Key::F(11)), k(Key::F(12))],
            ),
            // Alt-x arrives as Esc then x
            (
                b"\x1bx\x1bX\x1b\xc3\xa9",
                &[k(Key::Alt('x')), k(Key::Alt('X')), k(Key::Alt('é'))],
            ),
            (
                b"\x1b\x7f\x1b\r",
                &[
                    k(Key::Mod(Mods::ALT, NamedKey::Backspace)),
                    k(Key::Mod(Mods::ALT, NamedKey::Enter)),
                ],
            ),
            // Esc then Esc, or a control character, is Esc and then that
            (b"\x1b\x1b", &[k(Key::Esc), k(Key::Esc)]),
            (b"\x1b\x03", &[k(Key::Esc), k(Key::Ctrl('c'))]),
        ];
        for (bytes, want) in cases {
            assert_eq!(keys(bytes), *want, "{bytes:?}");
        }
    }

    #[test]
    fn reads_the_modifiers_held_with_a_key() {
        let k = |m, n| Event::Key(Key::Mod(m, n));
        assert_eq!(
            keys(
                b"\x1b[1;5D\x1b[1;2A\x1b[1;3H\x1b[1;6F\x1b[3;5~\x1b[5;2~\x1b[6;7~\x1b[15;2~\x1b[1;5P\x1b[1;9C\x1b[1;1B"
            ),
            [
                k(Mods::CTRL, NamedKey::Left),
                k(Mods::SHIFT, NamedKey::Up),
                k(Mods::ALT, NamedKey::Home),
                k(Mods::CTRL | Mods::SHIFT, NamedKey::End),
                k(Mods::CTRL, NamedKey::Delete),
                k(Mods::SHIFT, NamedKey::PageUp),
                k(Mods::CTRL | Mods::ALT, NamedKey::PageDown),
                k(Mods::SHIFT, NamedKey::F(5)),
                k(Mods::CTRL, NamedKey::F(1)),
                // Meta is taken as Alt, and a modifier of 1 means none
                k(Mods::ALT, NamedKey::Right),
                Event::Key(Key::Down),
            ]
        );
    }

    #[test]
    fn keys_come_apart_into_modifiers_and_the_key() {
        let ctrl_left = Key::Mod(Mods::CTRL, NamedKey::Left);
        assert_eq!((ctrl_left.mods(), ctrl_left.unmodified()), (Mods::CTRL, Key::Left));
        assert_eq!(
            (Key::Alt('b').mods(), Key::Alt('b').unmodified()),
            (Mods::ALT, Key::Char('b'))
        );
        assert_eq!(
            (Key::Ctrl('r').mods(), Key::BackTab.unmodified()),
            (Mods::CTRL, Key::Tab)
        );
        assert_eq!((Key::Up.mods(), Key::Up.unmodified()), (Mods::NONE, Key::Up));
        assert_eq!(Key::modified(NamedKey::Tab, Mods::SHIFT), Key::BackTab);
        assert_eq!(Key::modified(NamedKey::F(3), Mods::NONE), Key::F(3));
        assert_eq!(
            Key::modified(NamedKey::Tab, Mods::CTRL),
            Key::Mod(Mods::CTRL, NamedKey::Tab)
        );
        let mut m = Mods::NONE;
        m |= Mods::SHIFT;
        assert!(m.shift() && !m.alt() && !m.is_empty() && Mods::default().is_empty());
    }

    #[test]
    fn lets_go_of_what_it_doesnt_know() {
        // unknown sequences, stray control and continuation bytes, and broken UTF-8
        assert_eq!(
            keys(b"\x1b[99~\x1b[5;3X\x1bOZ\x1c\x80\xc3("),
            [Event::Key(Key::Char('('))]
        );
        // unfinished sequences give up as Esc when nothing more is coming
        assert_eq!(parse(b"\x1bO", false), None);
        assert_eq!(parse(b"\x1bO", true), Some((Some(Event::Key(Key::Esc)), 1)));
        assert_eq!(parse(b"\x1b[12", true), Some((Some(Event::Key(Key::Esc)), 1)));
        assert_eq!(parse(b"\xe2\x9c", true), Some((None, 2)));
        assert_eq!(parse(b"\x1b[M", true), Some((None, 3)));
        assert_eq!(parse(b"", true), None);
    }

    #[test]
    fn waits_for_the_rest_of_a_sequence_unless_done() {
        assert_eq!(parse(b"\x1b", false), None);
        assert_eq!(parse(b"\x1b[1", false), None);
        assert_eq!(parse("✦".as_bytes().get(..2).unwrap(), false), None);
        assert_eq!(parse(b"\x1b", true), Some((Some(Event::Key(Key::Esc)), 1)));
    }

    #[test]
    fn reads_legacy_mouse_reports_without_typing_their_bytes() {
        // a click at column 81, row 1: the x byte is `q`
        assert_eq!(
            keys(b"\x1b[M qA\x1b[M#qA\x1b[Ma!!"),
            [
                Event::Mouse(Mouse::new(MouseKind::Click, 80, 32)),
                Event::Mouse(Mouse::new(MouseKind::Release, 80, 32)),
                Event::Mouse(Mouse::new(MouseKind::ScrollDown, 0, 0)),
            ]
        );
        assert_eq!(parse(b"\x1b[M q", false), None);
    }

    #[test]
    fn reads_every_kind_of_mouse_report() {
        let at = |kind| Event::Mouse(Mouse::new(kind, 1, 1));
        assert_eq!(
            keys(b"\x1b[<2;2;2M\x1b[<64;2;2M\x1b[<2;2;2m\x1b[<1;2;2M\x1b[<32;2;2M\x1b[<35;2;2M\x1b[<0;;2M"),
            [
                at(MouseKind::RightClick),
                at(MouseKind::ScrollUp),
                at(MouseKind::Release),
                at(MouseKind::MiddleClick),
                at(MouseKind::Drag),
            ]
        );
        // with Shift, Alt and Ctrl held
        let mut held = Mouse::new(MouseKind::Click, 1, 1);
        held.mods = Mods::SHIFT | Mods::ALT | Mods::CTRL;
        assert_eq!(keys(b"\x1b[<28;2;2M"), [Event::Mouse(held)]);
    }

    #[test]
    fn reads_mouse_reports() {
        assert_eq!(
            keys(b"\x1b[<0;5;3M\x1b[<0;5;3m\x1b[<65;1;1M"),
            [
                Event::Mouse(Mouse::new(MouseKind::Click, 4, 2)),
                Event::Mouse(Mouse::new(MouseKind::Release, 4, 2)),
                Event::Mouse(Mouse::new(MouseKind::ScrollDown, 0, 0)),
            ]
        );
    }

    #[test]
    fn reads_a_paste_whole_even_through_a_pause() {
        let pasted = b"\x1b[200~one\r\ntwo\rthree\x1b[A\x1b[201~x";
        assert_eq!(
            keys(pasted),
            [Event::Paste("one\ntwo\nthree\x1b[A".into()), Event::Key(Key::Char('x'))]
        );
        // half a paste is waited for, pause or not
        assert_eq!(parse(b"\x1b[I", true), Some((Some(Event::Focus(true)), 3)));
        assert_eq!(parse(b"\x1b[O", true), Some((Some(Event::Focus(false)), 3)));
        assert_eq!(parse(b"\x1b[200~half\n", true), None);
        assert_eq!(parse(b"\x1b[200~half\x1b[20", false), None);
        // and an empty one is still a paste
        assert_eq!(
            parse(b"\x1b[200~\x1b[201~", true),
            Some((Some(Event::Paste(String::new())), 12))
        );
    }

    #[test]
    fn a_sequence_broken_off_is_esc_and_what_follows_is_read_again() {
        assert_eq!(keys(b"\x1b[\x1b[A"), [Event::Key(Key::Esc), Event::Key(Key::Up)]);
        assert_eq!(keys(b"\x1b[1;\x1b[B"), [Event::Key(Key::Esc), Event::Key(Key::Down)]);
        assert_eq!(
            keys(b"\x1b[1\rx"),
            [Event::Key(Key::Esc), Event::Key(Key::Enter), Event::Key(Key::Char('x'))]
        );
        // intermediates come between the parameters and the final byte
        assert_eq!(keys(b"\x1b[2 q"), []);
    }

    /// Everything a decoder makes of `pieces`, fed in one at a time, with a pause at the end.
    fn decoded(pieces: &[&[u8]]) -> Vec<Event> {
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        for (i, piece) in pieces.iter().enumerate() {
            decoder.push(piece);
            while let Some(event) = decoder.next(i + 1 == pieces.len()) {
                out.extend(event);
            }
        }
        out
    }

    #[test]
    fn a_decoder_reads_whatever_pieces_bytes_come_in() {
        let all = b"a\x1b[A\x1b[200~one\r\ntwo\x1b[201~\x1b";
        let want = [
            Event::Key(Key::Char('a')),
            Event::Key(Key::Up),
            Event::Paste("one\ntwo".into()),
            Event::Key(Key::Esc),
        ];
        assert_eq!(decoded(&[all]), want);
        // a byte at a time, the end of the paste split between reads included
        let bytes: Vec<&[u8]> = all.chunks(1).collect();
        assert_eq!(decoded(&bytes), want);
        assert_eq!(
            decoded(&[b"\x1b[200~x\x1b[2", b"01~y"]),
            [Event::Paste("x".into()), Event::Key(Key::Char('y'))]
        );
    }

    #[test]
    fn a_big_paste_is_read_in_one_pass() {
        let text = "pasted line\n".repeat(100_000);
        let mut decoder = Decoder::new();
        decoder.push(b"\x1b[200~");
        let started = std::time::Instant::now();
        for b in text.as_bytes() {
            decoder.push(std::slice::from_ref(b));
            assert_eq!(decoder.next(false), None);
        }
        decoder.push(PASTE_END);
        assert_eq!(decoder.next(false), Some(Some(Event::Paste(text))));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_paste_that_never_ends_is_taken_once_it_goes_quiet_or_grows_too_big() {
        let mut decoder = Decoder::new();
        decoder.push(b"\x1b[200~half");
        assert_eq!(decoder.next(true), None, "a short pause is waited out");
        decoder.paste_wait = Duration::ZERO;
        assert_eq!(decoder.next(false), None, "only a pause ends it");
        assert_eq!(decoder.next(true), Some(Some(Event::Paste("half".into()))));
        decoder.push(b"\x03");
        assert_eq!(decoder.next(false), Some(Some(Event::Key(Key::Ctrl('c')))));

        let mut decoder = Decoder::new();
        decoder.push(b"\x1b[200~");
        decoder.push(&vec![b'x'; MAX_PASTE]);
        let Some(Some(Event::Paste(text))) = decoder.next(false) else {
            panic!("not taken")
        };
        assert_eq!(text.len(), MAX_PASTE);
        decoder.push(b"\x1b[201~q");
        assert_eq!(decoder.next(false), Some(None));
        assert_eq!(decoder.next(false), Some(Some(Event::Key(Key::Char('q')))));
    }

    #[test]
    fn reads_four_byte_characters() {
        assert_eq!(keys("🎉".as_bytes()), [Event::Key(Key::Char('🎉'))]);
    }
}

//! Turning the bytes a terminal sends into keys and mouse events.

/// A key someone pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character, space included.
    Char(char),
    /// Ctrl and a letter, as the lowercase letter. Ctrl-C is `Ctrl('c')`.
    Ctrl(char),
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
    /// A function key, F1 to F12.
    F(u8),
}

/// What the mouse did. Only reported when [`App::mouse`](super::App::mouse) says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    /// A left click.
    Click,
    /// A right click.
    RightClick,
    /// The wheel, rolled up.
    ScrollUp,
    /// The wheel, rolled down.
    ScrollDown,
}

/// A mouse event, at a cell counted from 0 at the top left. Check which panel it landed in
/// with [`Rect::contains`](super::Rect::contains).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    /// What happened.
    pub kind: MouseKind,
    /// The column.
    pub x: usize,
    /// The row.
    pub y: usize,
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
            // Esc then something else (Alt-x): the Esc on its own, the rest read after
            Some(_) => key(Key::Esc, 1),
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
    let Some(end) = bytes.iter().skip(2).position(|b| (0x40..=0x7E).contains(b)) else {
        // a pause halfway through: give up on it as Esc, and read the rest as typed
        return if done {
            Some((Some(Event::Key(Key::Esc)), 1))
        } else {
            None
        };
    };
    let end = end + 2;
    let params = std::str::from_utf8(&bytes[2..end]).unwrap_or("");
    let n = end + 1;
    let key = match (bytes[end], params) {
        (b'M' | b'm', p) if p.starts_with('<') => return Some((mouse(&p[1..], bytes[end] == b'M'), n)),
        (b'A', _) => Key::Up,
        (b'B', _) => Key::Down,
        (b'C', _) => Key::Right,
        (b'D', _) => Key::Left,
        (b'H', _) => Key::Home,
        (b'F', _) => Key::End,
        (b'Z', _) => Key::BackTab,
        // modifiers come after a `;`, and are let go
        (b'~', p) => match p.split(';').next().unwrap_or("") {
            "1" | "7" => Key::Home,
            "4" | "8" => Key::End,
            "3" => Key::Delete,
            "5" => Key::PageUp,
            "6" => Key::PageDown,
            code => match code.parse::<u8>() {
                Ok(c @ 11..=15) => Key::F(c - 10),
                Ok(c @ 17..=21) => Key::F(c - 11),
                Ok(c @ 23..=24) => Key::F(c - 12),
                _ => return Some((None, n)),
            },
        },
        _ => return Some((None, n)),
    };
    Some((Some(Event::Key(key)), n))
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
    let text = String::from_utf8_lossy(&bytes[start..start + len]);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Some((Some(super::Event::Paste(text)), start + len + PASTE_END.len()))
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

/// `button;x;y` from an SGR mouse report (1-based). Releases and drags are let go.
fn mouse(params: &str, pressed: bool) -> Option<super::Event> {
    let mut it = params.split(';').map(|p| p.parse::<usize>().ok());
    let (button, x, y) = (it.next()??, it.next()??, it.next()??);
    let kind = match button {
        64 => MouseKind::ScrollUp,
        65 => MouseKind::ScrollDown,
        0 if pressed => MouseKind::Click,
        2 if pressed => MouseKind::RightClick,
        _ => return None,
    };
    Some(super::Event::Mouse(Mouse {
        kind,
        x: x.saturating_sub(1),
        y: y.saturating_sub(1),
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
                Event::Key(Key::Right),
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
            (b"\x1bx", &[k(Key::Esc), k(Key::Char('x'))]),
        ];
        for (bytes, want) in cases {
            assert_eq!(keys(bytes), *want, "{bytes:?}");
        }
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
                Event::Mouse(Mouse {
                    kind: MouseKind::Click,
                    x: 80,
                    y: 32
                }),
                Event::Mouse(Mouse {
                    kind: MouseKind::ScrollDown,
                    x: 0,
                    y: 0
                }),
            ]
        );
        assert_eq!(parse(b"\x1b[M q", false), None);
    }

    #[test]
    fn reads_every_kind_of_mouse_report() {
        let at = |kind| Event::Mouse(Mouse { kind, x: 1, y: 1 });
        assert_eq!(
            keys(b"\x1b[<2;2;2M\x1b[<64;2;2M\x1b[<2;2;2m\x1b[<1;2;2M\x1b[<0;;2M"),
            [at(MouseKind::RightClick), at(MouseKind::ScrollUp)]
        );
    }

    #[test]
    fn reads_mouse_reports() {
        assert_eq!(
            keys(b"\x1b[<0;5;3M\x1b[<0;5;3m\x1b[<65;1;1M"),
            [
                Event::Mouse(Mouse {
                    kind: MouseKind::Click,
                    x: 4,
                    y: 2
                }),
                Event::Mouse(Mouse {
                    kind: MouseKind::ScrollDown,
                    x: 0,
                    y: 0
                }),
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
        assert_eq!(parse(b"\x1b[200~half\n", true), None);
        assert_eq!(parse(b"\x1b[200~half\x1b[20", false), None);
        // and an empty one is still a paste
        assert_eq!(
            parse(b"\x1b[200~\x1b[201~", true),
            Some((Some(Event::Paste(String::new())), 12))
        );
    }

    #[test]
    fn reads_four_byte_characters() {
        assert_eq!(keys("🎉".as_bytes()), [Event::Key(Key::Char('🎉'))]);
    }
}

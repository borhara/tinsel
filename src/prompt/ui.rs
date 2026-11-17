//! What every question shares: raw mode, keys, drawing, and reading from a pipe.

use super::*;

/// Raw keyboard input and a hidden cursor for as long as it lives, with every live display
/// held still meanwhile so the question has the screen and the keys to itself.
pub(super) struct Raw {
    // raw mode goes off in drop, before the displays start watching stdin again
    mode: Option<RawMode>,
    _paused: crate::display::Paused,
    /// The handlers for hangups and kills from before the question, to put back.
    termination: [Option<usize>; 2],
}

/// What a question writes on its way out: pastes no longer marked out, and the cursor back.
const LEAVE: &str = "\x1b[?2004l\x1b[?25h";

impl Raw {
    fn enter() -> io::Result<Raw> {
        // decide the theme first: finding out may mean asking the terminal, which needs stdin
        crate::style::theme();
        // before raw mode, so each display's focus watch has handed the terminal back first
        let paused = crate::display::pause_all();
        // the settings from before raw mode, for a hangup, a kill or an exit to put back
        crate::term::remember_terminal();
        let mode = match RawMode::enter() {
            Ok(mode) => mode,
            Err(e) => {
                crate::term::forget_terminal();
                return Err(e);
            }
        };
        // what's left behind goes to stdout, so only when that's the terminal too: piped, it
        // would end up in what the program printed
        let leave = if io::stdout().is_terminal() { LEAVE } else { "" };
        let termination = crate::term::guard_termination(leave);
        // the cursor hidden, and pastes marked out, so a line break in one isn't Enter
        eprint!("\x1b[?25l\x1b[?2004h");
        Ok(Raw {
            mode: Some(mode),
            _paused: paused,
            termination,
        })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        eprint!("{LEAVE}");
        let _ = io::stderr().flush();
        // the terminal put back before the guards come off, so a kill meanwhile still does it
        drop(self.mode.take());
        crate::term::unguard_termination(std::mem::take(&mut self.termination));
        crate::term::forget_terminal();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Char(char),
    /// A character of a paste: typed into an answer or a filter, but never a key that
    /// answers, moves or ticks.
    Pasted(char),
    /// Ctrl and a letter, as the letter.
    Ctrl(char),
    Esc,
    /// Alt-← or Ctrl-←, or Alt-b: back a word.
    WordLeft,
    /// Alt-→ or Ctrl-→, or Alt-f: on a word.
    WordRight,
    /// Ctrl-C.
    Interrupt,
    Other,
}

/// What a terminal sends: a key, or text pasted all at once.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Typed {
    Key(Key),
    Paste(String),
}

/// The longest escape sequence read as one: anything longer is noise, not a key.
pub(super) const LONGEST_SEQUENCE: usize = 32;
/// Pauses (of 0.1s each) a paste may take to arrive in full, over a slow link, before what
/// came is taken as all of it.
pub(super) const PASTE_PATIENCE: usize = 30;

/// The next key or paste, from bytes that `read` gives one at a time (`None` after a pause).
pub(super) fn read_key(read: &mut dyn FnMut() -> io::Result<Option<u8>>) -> io::Result<Typed> {
    let first = loop {
        // reads time out every 0.1s, which is what lets a lone Esc be told apart
        if let Some(b) = read()? {
            break b;
        }
    };
    let key = match first {
        3 => Key::Interrupt,
        b'\r' | b'\n' => Key::Enter,
        b'\t' => Key::Tab,
        b' ' => Key::Space,
        127 | 8 => Key::Backspace,
        27 => return escape(read),
        b @ 1..=26 => Key::Ctrl((b'a' + b - 1) as char),
        b if b.is_ascii_graphic() => Key::Char(b as char),
        b if b >= 0xC0 => {
            let len = if b >= 0xF0 {
                4
            } else if b >= 0xE0 {
                3
            } else {
                2
            };
            let mut buf = vec![b];
            for _ in 1..len {
                buf.extend(read()?);
            }
            String::from_utf8(buf)
                .ok()
                .and_then(|s| s.chars().next())
                .map_or(Key::Other, Key::Char)
        }
        _ => Key::Other,
    };
    Ok(Typed::Key(key))
}

/// What follows an Esc: nothing (Esc itself), a sequence the terminal sends for a key or a
/// paste, or a key pressed with Alt (Option on a Mac), which terminals send as Esc then the key.
pub(super) fn escape(read: &mut dyn FnMut() -> io::Result<Option<u8>>) -> io::Result<Typed> {
    let key = match read()? {
        None | Some(27) => Key::Esc,
        Some(b'[') => return csi(read),
        Some(b'O') => match read()? {
            Some(b'A') => Key::Up,
            Some(b'B') => Key::Down,
            Some(b'C') => Key::Right,
            Some(b'D') => Key::Left,
            Some(b'H') => Key::Home,
            Some(b'F') => Key::End,
            _ => Key::Other,
        },
        // Alt-← and Alt-→ in most Mac terminals, and readline's word keys everywhere
        Some(b'b') => Key::WordLeft,
        Some(b'f') => Key::WordRight,
        Some(127 | 8) => Key::Ctrl('w'),
        // any other Alt key: not typing, and not a reason to give up on the question
        Some(_) => Key::Other,
    };
    Ok(Typed::Key(key))
}

/// `ESC [`, then parameters, then one final byte: arrows and the keys around them, with or
/// without modifiers (`ESC [ 1 ; 5 C` is Ctrl-→), and the start of a paste.
pub(super) fn csi(read: &mut dyn FnMut() -> io::Result<Option<u8>>) -> io::Result<Typed> {
    let mut params = String::new();
    let last = loop {
        match read()? {
            Some(b @ 0x40..=0x7E) => break b,
            Some(b @ 0x20..=0x3F) if params.len() < LONGEST_SEQUENCE => params.push(b as char),
            // cut short, or not a sequence after all: nothing anyone meant to type
            _ => return Ok(Typed::Key(Key::Other)),
        }
    };
    let mut parts = params.split(';');
    let code = parts.next().unwrap_or("");
    // 3 is Alt, 5 Ctrl, and so on: either moves a word at a time
    let modified = parts.next().is_some_and(|m| m != "1");
    let key = match (last, code) {
        (b'C', _) if modified => Key::WordRight,
        (b'D', _) if modified => Key::WordLeft,
        (b'A', _) => Key::Up,
        (b'B', _) => Key::Down,
        (b'C', _) => Key::Right,
        (b'D', _) => Key::Left,
        (b'H', _) => Key::Home,
        (b'F', _) => Key::End,
        (b'~', "1" | "7") => Key::Home,
        (b'~', "4" | "8") => Key::End,
        (b'~', "3") => Key::Delete,
        (b'~', "5") => Key::PageUp,
        (b'~', "6") => Key::PageDown,
        (b'~', "200") => return paste(read),
        _ => Key::Other,
    };
    Ok(Typed::Key(key))
}

/// What comes after `ESC [ 200 ~`, up to `ESC [ 201 ~`: pasted text, all of it. It's waited
/// for through a pause or two, since a big paste can arrive in pieces.
pub(super) fn paste(read: &mut dyn FnMut() -> io::Result<Option<u8>>) -> io::Result<Typed> {
    const END: &[u8] = b"\x1b[201~";
    let mut bytes = Vec::new();
    let mut pauses = 0;
    while !bytes.ends_with(END) {
        match read()? {
            Some(b) => bytes.push(b),
            None if pauses < PASTE_PATIENCE => pauses += 1,
            None => break,
        }
    }
    if bytes.ends_with(END) {
        bytes.truncate(bytes.len() - END.len());
    }
    Ok(Typed::Paste(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Pasted text as [`Key::Pasted`] characters, for a one-line answer: each line break or tab
/// becomes a space, and a line break at the end is left off. A paste never answers the
/// question: `y` in one isn't yes, `j` doesn't move and a space doesn't tick an option.
pub(super) fn pasted_keys(text: &str) -> Vec<Key> {
    let text = text.trim_end_matches(['\r', '\n']).replace("\r\n", "\n");
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(Key::Pasted(' ')),
            c if c.is_control() => None,
            c => Some(Key::Pasted(c)),
        })
        .collect()
}

/// Redraws a block of lines on stderr in place.
#[derive(Default)]
pub(super) struct Frame {
    pub(super) drawn: usize,
    /// What was last drawn, kept for tests.
    pub(super) lines: Vec<String>,
    /// Keep the lines without printing them.
    offscreen: bool,
}

impl Drop for Ui {
    fn drop(&mut self) {
        if self.record && !self.frame.lines.is_empty() {
            let lines = self.frame.lines.iter().map(|l| text::strip_ansi(l).trim().to_string());
            SCRIPT.with(|s| s.borrow_mut().as_mut().map(|s| s.answered.extend(lines)));
        }
    }
}

impl Frame {
    pub(super) fn draw(&mut self, lines: &[String]) {
        self.lines = lines.to_vec();
        if self.offscreen {
            return;
        }
        let (cols, rows) = crate::term::stderr_size();
        let (s, drawn) = frame(self.drawn, lines, cols, rows);
        eprint!("{s}");
        let _ = io::stderr().flush();
        self.drawn = drawn;
    }
}

/// The escape codes and text that draw `lines` over the `drawn` rows of the last frame on a
/// terminal of `cols`×`rows`, and how many rows they take up.
///
/// A line wider than the terminal wraps onto the rows below, so rows are counted, not lines:
/// otherwise a long answer would leave a stale copy behind on every key. Only the lines that
/// fit on screen are drawn, since the cursor can't move back above the top of it to redraw
/// the rest.
pub(super) fn frame(drawn: usize, lines: &[String], cols: usize, rows: usize) -> (String, usize) {
    let mut s = if drawn > 0 {
        format!("\x1b[{drawn}F")
    } else {
        "\r".to_string()
    };
    let room = rows.saturating_sub(1).max(1);
    let mut used = 0;
    for line in lines {
        let tall = text::width(line).div_ceil(cols.max(1)).max(1);
        if used > 0 && used + tall > room {
            break;
        }
        s += line;
        s += "\x1b[K\n";
        used += tall;
    }
    s += "\x1b[J";
    (s, used)
}

/// Where a question reads keys and draws: the terminal in raw mode, or a script in tests.
pub(super) struct Ui {
    keys: Box<dyn FnMut() -> io::Result<Key>>,
    pub(super) frame: Frame,
    _raw: Option<Raw>,
    /// Hand the last lines to the running [`Script`] when done.
    record: bool,
}

impl Ui {
    /// The script's keys when one is running on this thread, otherwise the terminal.
    pub(super) fn open() -> io::Result<Ui> {
        if !scripted() {
            return Ui::terminal();
        }
        Ok(Ui {
            keys: Box::new(|| {
                next_scripted()
                    .map(Key::from)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "the script ran out of keys"))
            }),
            frame: Frame {
                offscreen: true,
                ..Frame::default()
            },
            _raw: None,
            record: true,
        })
    }

    fn terminal() -> io::Result<Ui> {
        let raw = Raw::enter()?;
        // a paste, handed out a key at a time
        let mut pasted = std::collections::VecDeque::new();
        Ok(Ui {
            keys: Box::new(move || {
                loop {
                    if let Some(key) = pasted.pop_front() {
                        return Ok(key);
                    }
                    match read_key(&mut read_byte)? {
                        Typed::Key(key) => return Ok(key),
                        Typed::Paste(text) => pasted.extend(pasted_keys(&text)),
                    }
                }
            }),
            frame: Frame::default(),
            _raw: Some(raw),
            record: false,
        })
    }

    /// Keys typed from a list, drawing nowhere; running out of keys is an error.
    #[cfg(test)]
    pub(super) fn script(keys: &[Key]) -> Ui {
        let mut keys = Vec::from(keys).into_iter();
        Ui {
            keys: Box::new(move || keys.next().ok_or_else(|| io::ErrorKind::UnexpectedEof.into())),
            frame: Frame {
                offscreen: true,
                ..Frame::default()
            },
            _raw: None,
            record: false,
        }
    }

    pub(super) fn key(&mut self) -> io::Result<Key> {
        (self.keys)()
    }

    pub(super) fn draw(&mut self, lines: &[String]) {
        self.frame.draw(lines);
    }

    /// Clear the question away, for the one before it to be asked again.
    pub(super) fn back(&mut self) -> Error {
        self.frame.draw(&[]);
        Error::Back
    }

    /// The last thing drawn, without colour.
    #[cfg(test)]
    pub(super) fn shown(&self) -> Vec<String> {
        self.frame.lines.iter().map(|l| text::strip_ansi(l)).collect()
    }
}

pub(super) fn ask(question: &str) -> String {
    format!("  {}  {}", paint(ROSE, "?"), bold(&paint(TEXT, question)))
}

pub(super) fn answered(question: &str, answer: &str) -> String {
    format!(
        "  {}  {}  {}",
        paint(ROSE, "●"),
        paint(TEXT, question),
        paint(BLUSH, answer)
    )
}

pub(super) fn cancel(frame: &mut Frame, question: &str) -> Error {
    frame.draw(&[format!(
        "  {}  {}  {}",
        paint(RED, "✕"),
        paint(TEXT, question),
        paint(MUTED, "cancelled")
    )]);
    Error::Cancelled
}

/// "  ← back", for the end of a question's line, when going back is on.
pub(super) fn back_hint(back: bool) -> String {
    if back {
        format!("  {}", paint(MUTED, "← back"))
    } else {
        String::new()
    }
}

pub(super) fn keys(hint: &str) -> String {
    format!("    {}", paint(MUTED, hint))
}

pub(super) fn problem(why: &str) -> String {
    format!("     {} {}", paint(RED, "✕"), paint(mix(RED, SOFT, 0.3), why))
}

/// Whether there's someone to ask: a terminal on stdin and on stderr, where the question is
/// drawn, or a [`script`] of keys. With stderr sent elsewhere (`2>/dev/null`) the question
/// couldn't be seen, so it isn't asked.
pub(super) fn interactive() -> bool {
    #[cfg(test)]
    if let Some(terminal) = crate::output::pretended_terminal() {
        return scripted() || terminal;
    }
    scripted() || (io::stdin().is_terminal() && io::stderr().is_terminal())
}

/// Whether stdin is a terminal while the question can't be shown on stderr.
pub(super) fn typing_blind() -> bool {
    #[cfg(test)]
    if crate::output::pretended_terminal().is_some() {
        return false;
    }
    io::stdin().is_terminal() && !io::stderr().is_terminal()
}

pub(super) fn need_terminal() -> Result<()> {
    if interactive() {
        Ok(())
    } else {
        Err(Error::NotATerminal)
    }
}

/// A line read from a pipe or file, for when there's no one to ask. The end of the input
/// reads as an empty line.
pub(super) fn read_piped_line() -> Result<String> {
    Ok(read_piped()?.unwrap_or_default())
}

/// A line read from a pipe or file, or `None` at the end of the input.
pub(super) fn read_piped() -> Result<Option<String>> {
    // stdin is a terminal but stderr isn't: someone is there, but would be typing an answer
    // to a question they can't see
    if typing_blind() {
        return Err(Error::NotATerminal);
    }
    let mut s = String::new();
    if io::stdin().read_line(&mut s)? == 0 {
        return Ok(None);
    }
    Ok(Some(s.trim_end_matches(['\r', '\n']).to_string()))
}

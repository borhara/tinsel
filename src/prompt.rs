//! Questions: yes/no, a line of text, a password, a number, a date, one of a list, several
//! of a list, or a longer answer in the user's editor.
//!
//! Drawn on stderr, so stdout stays clean for piping. Each question collapses to a single
//! `●  question  answer` line once answered.
//!
//! Every question returns a [`Result`], whose error says what happened in a way you can
//! match on: [`Error::Cancelled`] when someone presses Ctrl-C or Esc, [`Error::NotATerminal`]
//! when there's no one to ask. Without a terminal on stdin, `confirm` returns its default and
//! the text questions (`input`, `number`, `date`) read a line, so scripts can still drive
//! them; an answer read that way that fails validation is an [`Error::Invalid`].
//!
//! ```no_run
//! use tinsel::prompt;
//!
//! fn main() -> Result<(), prompt::Error> {
//!     let name = prompt::input("Project name", Some("my-app"))?;
//!     let target = prompt::select("Deploy to", &["staging", "production"])?;
//!     if prompt::confirm("Go?", true)? {
//!         tinsel::success(&format!("deploying {name} to target #{target}"));
//!     }
//!     Ok(())
//! }
//! ```
//!
//! For more control over a text question (a placeholder, validation), build an [`Input`]:
//!
//! ```no_run
//! use tinsel::prompt::Input;
//!
//! let port = Input::new("Port")
//!     .placeholder("8080")
//!     .validate(|s| match s.parse::<u16>() {
//!         Ok(p) if p >= 1024 => Ok(()),
//!         _ => Err("pick a port from 1024 to 65535".into()),
//!     })
//!     .ask()?;
//! # Ok::<(), tinsel::prompt::Error>(())
//! ```

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::str::FromStr;

use crate::style::palette::{BLUSH, MUTED, RED, ROSE, SOFT, TEXT, TRACK};
use crate::style::{bold, mix, paint, paint_bg, underline};
use crate::term::{RawMode, read_byte};
use crate::text;

/// Why a question didn't get an answer.
#[derive(Debug)]
pub enum Error {
    /// The person pressed Ctrl-C or Esc. Exiting with status 130 is the convention.
    Cancelled,
    /// There's no terminal to ask on (stdin is a pipe or a file), and this question has no
    /// sensible non-interactive answer.
    NotATerminal,
    /// A list question was given nothing to choose from.
    NoOptions,
    /// An answer read without a terminal (from a pipe or file) didn't pass validation. On a
    /// terminal the person is simply asked again.
    Invalid(String),
    /// Reading the keyboard, setting up the terminal or running the editor failed.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cancelled => f.write_str("cancelled"),
            Error::NotATerminal => f.write_str("this question needs a terminal to answer it"),
            Error::NoOptions => f.write_str("there was nothing to choose from"),
            Error::Invalid(why) => write!(f, "invalid answer: {why}"),
            Error::Io(e) => write!(f, "couldn't talk to the terminal: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// What every question returns.
pub type Result<T> = std::result::Result<T, Error>;

/// Raw keyboard input and a hidden cursor for as long as it lives.
struct Raw {
    _mode: RawMode,
}

impl Raw {
    fn enter() -> io::Result<Raw> {
        // decide the theme first: finding out may mean asking the terminal, which needs stdin
        crate::style::theme();
        let mode = RawMode::enter()?;
        eprint!("\x1b[?25l");
        Ok(Raw { _mode: mode })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        eprint!("\x1b[?25h");
        let _ = io::stderr().flush();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
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
    /// Ctrl and a letter, as the letter.
    Ctrl(char),
    Esc,
    /// Ctrl-C.
    Interrupt,
    Other,
}

fn read_key() -> io::Result<Key> {
    let first = loop {
        // reads time out every 0.1s, which is what lets a lone Esc be told apart
        if let Some(b) = read_byte()? {
            break b;
        }
    };
    Ok(match first {
        3 => Key::Interrupt,
        b'\r' | b'\n' => Key::Enter,
        b'\t' => Key::Tab,
        b' ' => Key::Space,
        127 | 8 => Key::Backspace,
        27 => match read_byte()? {
            Some(b'[' | b'O') => match read_byte()? {
                Some(b'A') => Key::Up,
                Some(b'B') => Key::Down,
                Some(b'C') => Key::Right,
                Some(b'D') => Key::Left,
                Some(b'H') => Key::Home,
                Some(b'F') => Key::End,
                // ESC [ n ~
                Some(n @ b'1'..=b'8') => {
                    let mut code = vec![n];
                    while let Some(b) = read_byte()? {
                        if b == b'~' {
                            break;
                        }
                        code.push(b);
                    }
                    match code.as_slice() {
                        b"1" | b"7" => Key::Home,
                        b"4" | b"8" => Key::End,
                        b"3" => Key::Delete,
                        b"5" => Key::PageUp,
                        b"6" => Key::PageDown,
                        _ => Key::Other,
                    }
                }
                _ => Key::Other,
            },
            _ => Key::Esc,
        },
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
                buf.extend(read_byte()?);
            }
            String::from_utf8(buf)
                .ok()
                .and_then(|s| s.chars().next())
                .map_or(Key::Other, Key::Char)
        }
        _ => Key::Other,
    })
}

/// Redraws a block of lines on stderr in place.
#[derive(Default)]
struct Frame {
    drawn: usize,
}

impl Frame {
    fn draw(&mut self, lines: &[String]) {
        let mut s = if self.drawn > 0 {
            format!("\x1b[{}F", self.drawn)
        } else {
            "\r".to_string()
        };
        for line in lines {
            s += line;
            s += "\x1b[K\n";
        }
        s += "\x1b[J";
        eprint!("{s}");
        let _ = io::stderr().flush();
        self.drawn = lines.len();
    }
}

fn ask(question: &str) -> String {
    format!("  {}  {}", paint(ROSE, "?"), bold(&paint(TEXT, question)))
}

fn answered(question: &str, answer: &str) -> String {
    format!(
        "  {}  {}  {}",
        paint(ROSE, "●"),
        paint(TEXT, question),
        paint(BLUSH, answer)
    )
}

fn cancel(frame: &mut Frame, question: &str) -> Error {
    frame.draw(&[format!(
        "  {}  {}  {}",
        paint(RED, "✕"),
        paint(TEXT, question),
        paint(MUTED, "cancelled")
    )]);
    Error::Cancelled
}

fn keys(hint: &str) -> String {
    format!("    {}", paint(MUTED, hint))
}

fn problem(why: &str) -> String {
    format!("     {} {}", paint(RED, "✕"), paint(mix(RED, SOFT, 0.3), why))
}

fn need_terminal() -> Result<()> {
    if io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(Error::NotATerminal)
    }
}

/// A line read from a pipe or file, for when there's no one to ask.
fn read_piped_line() -> Result<String> {
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

// MARK: - Yes or no

/// Yes or no. Enter takes the default.
pub fn confirm(question: &str, default: bool) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(default);
    }
    let _raw = Raw::enter()?;
    let mut frame = Frame::default();
    frame.draw(&[format!(
        "{}  {}",
        ask(question),
        paint(MUTED, if default { "Y/n" } else { "y/N" })
    )]);
    loop {
        let answer = match read_key()? {
            Key::Char('y' | 'Y') => true,
            Key::Char('n' | 'N') => false,
            Key::Enter => default,
            Key::Esc | Key::Interrupt => return Err(cancel(&mut frame, question)),
            _ => continue,
        };
        frame.draw(&[answered(question, if answer { "yes" } else { "no" })]);
        return Ok(answer);
    }
}

// MARK: - Text

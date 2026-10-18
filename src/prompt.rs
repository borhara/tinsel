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

type Validator<'a> = Box<dyn Fn(&str) -> std::result::Result<(), String> + 'a>;
type Completer<'a> = Box<dyn Fn(&str) -> Vec<String> + 'a>;

/// A text question with options: a default, a placeholder, hidden input, and validation that
/// asks again until the answer passes. [`input`] and [`password`] are shortcuts for the
/// common cases.
///
/// The answer can be edited as you'd expect: ←→ move, Home/End (or Ctrl-A/Ctrl-E) jump,
/// Ctrl-U clears, Ctrl-W deletes a word. With [`suggestions`](Input::suggestions),
/// [`complete`](Input::complete) or [`paths`](Input::paths), the best match shows dim
/// after the cursor; Tab takes it, and pressing Tab again moves through the others.
pub struct Input<'a> {
    question: String,
    default: Option<String>,
    placeholder: Option<String>,
    secret: bool,
    validate: Option<Validator<'a>>,
    complete: Option<Completer<'a>>,
}

impl<'a> Input<'a> {
    /// A text question.
    pub fn new(question: impl Into<String>) -> Self {
        Input {
            question: question.into(),
            default: None,
            placeholder: None,
            secret: false,
            validate: None,
            complete: None,
        }
    }

    /// Offer these answers as you type: those starting with what's typed (ignoring case).
    ///
    /// ```no_run
    /// let region = tinsel::prompt::Input::new("Region")
    ///     .suggestions(&["eu-west-1", "eu-west-2", "us-east-1"])
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    pub fn suggestions(self, options: &[&str]) -> Self {
        let options: Vec<String> = options.iter().map(|s| s.to_string()).collect();
        self.complete(move |typed| {
            let typed = typed.to_lowercase();
            options
                .iter()
                .filter(|o| o.to_lowercase().starts_with(&typed))
                .cloned()
                .collect()
        })
    }

    /// Offer whatever `complete` returns for the text typed so far, best first. Each
    /// suggestion is a whole answer, not just the rest of it.
    pub fn complete(mut self, complete: impl Fn(&str) -> Vec<String> + 'a) -> Self {
        self.complete = Some(Box::new(complete));
        self
    }

    /// Complete file and folder paths, like a shell: Tab fills in names from the folder
    /// typed so far (`~` is your home folder), and folders end in `/`.
    pub fn paths(self) -> Self {
        self.complete(complete_path)
    }

    /// The answer if Enter is pressed without typing anything, shown dim until you type.
    pub fn default(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Dim example text shown until you type. Unlike a default, it's never the answer.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Show what's typed as dots.
    pub fn password(mut self) -> Self {
        self.secret = true;
        self
    }

    /// Check the answer when Enter is pressed. An `Err` is shown under the question, which
    /// is asked again. The default is checked too, if it's used.
    pub fn validate(mut self, check: impl Fn(&str) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, value: &str) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(value))
    }

    /// Ask the question.
    pub fn ask(self) -> Result<String> {
        let resolve = |typed: String| {
            if typed.is_empty() {
                self.default.clone().unwrap_or_default()
            } else {
                typed
            }
        };
        if !io::stdin().is_terminal() {
            let value = resolve(read_piped_line()?);
            self.check(&value).map_err(Error::Invalid)?;
            return Ok(value);
        }
        let _raw = Raw::enter()?;
        let mut frame = Frame::default();
        let mut edit = LineEdit::default();
        let mut trouble: Option<String> = None;
        let dots = |n: usize| "•".repeat(n);
        // Tab cycles through what was offered for the text as it was before the first Tab
        let mut cycle: Option<(String, usize)> = None;
        loop {
            let offered: Vec<String> = match (&self.complete, &cycle) {
                (Some(c), Some((base, _))) => c(base),
                (Some(c), None) if edit.at == edit.text.len() && !self.secret => c(&edit.text),
                _ => Vec::new(),
            };
            let pick = cycle.as_ref().map_or(0, |(_, n)| *n);
            // the rest of the best suggestion, shown dim after the cursor
            let ghost = offered
                .get(pick)
                .filter(|_| cycle.is_none() && !edit.is_empty())
                .and_then(|s| {
                    let head: String = s.chars().take(edit.text.chars().count()).collect();
                    head.eq_ignore_ascii_case(&edit.text)
                        .then(|| s.chars().skip(head.chars().count()).collect::<String>())
                })
                .unwrap_or_default();
            let caret = paint(ROSE, "▏");
            let shown = if edit.is_empty() {
                let hint = self
                    .default
                    .as_deref()
                    .or(self.placeholder.as_deref())
                    .unwrap_or_default();
                format!("{caret}{}", paint(MUTED, hint))
            } else {
                let (before, after) = edit.split();
                let color = if self.secret { BLUSH } else { TEXT };
                let show = |s: &str| {
                    if self.secret {
                        dots(s.chars().count())
                    } else {
                        s.to_string()
                    }
                };
                format!(
                    "{}{caret}{}{}",
                    paint(color, &show(before)),
                    paint(color, &show(after)),
                    paint(MUTED, &ghost)
                )
            };
            let mut lines = vec![format!("{}  {shown}", ask(&self.question))];
            lines.extend(trouble.as_deref().map(problem));
            if offered.len() > 1 {
                let list: Vec<String> = offered
                    .iter()
                    .enumerate()
                    .take(6)
                    .map(|(i, s)| {
                        let here = cycle.is_some() && i == pick;
                        paint(if here { ROSE } else { MUTED }, s)
                    })
                    .collect();
                let more = if offered.len() > 6 {
                    format!("  +{}", offered.len() - 6)
                } else {
                    String::new()
                };
                lines.push(format!("     {}{}", list.join("  "), paint(MUTED, &more)));
            }
            frame.draw(&lines);
            match read_key()? {
                Key::Enter => {
                    let value = resolve(edit.text.clone());
                    match self.check(&value) {
                        Ok(()) => {
                            let shown = if self.secret {
                                dots(value.chars().count())
                            } else {
                                value.clone()
                            };
                            frame.draw(&[answered(&self.question, &shown)]);
                            return Ok(value);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Esc | Key::Interrupt => return Err(cancel(&mut frame, &self.question)),
                Key::Tab if !offered.is_empty() => {
                    let next = match &cycle {
                        Some((base, n)) => (base.clone(), (n + 1) % offered.len()),
                        None => (edit.text.clone(), 0),
                    };
                    edit.set(&offered[next.1]);
                    // with a single match there's nothing to cycle through: take it and carry on
                    cycle = (offered.len() > 1).then_some(next);
                    trouble = None;
                }
                // → at the end of the line takes the dim suggestion, as in a shell
                Key::Right if !ghost.is_empty() => {
                    edit.set(&offered[pick]);
                    trouble = None;
                }
                key => {
                    cycle = None;
                    if edit.key(key) {
                        trouble = None;
                    }
                }
            }
        }
    }
}

/// File and folder names that complete `typed`, as whole paths. Hidden files only show once
/// a `.` is typed.
fn complete_path(typed: &str) -> Vec<String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let expanded = match typed.strip_prefix('~') {
        Some(rest) if !home.is_empty() => format!("{home}{rest}"),
        _ => typed.to_string(),
    };
    let split = expanded.rfind(['/', std::path::MAIN_SEPARATOR]).map_or(0, |i| i + 1);
    let (dir, prefix) = expanded.split_at(split);
    let typed_dir = &typed[..typed.len() - prefix.len()];
    let Ok(entries) = std::fs::read_dir(if dir.is_empty() { "." } else { dir }) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().into_string().ok()?;
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let slash = if e.file_type().ok()?.is_dir() { "/" } else { "" };
            Some(format!("{typed_dir}{name}{slash}"))
        })
        .collect();
    found.sort();
    found
}

/// A line being typed, with a cursor that can move through it.
#[derive(Default)]
struct LineEdit {
    text: String,
    /// The cursor, as a byte index on a char boundary.
    at: usize,
}

impl LineEdit {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn split(&self) -> (&str, &str) {
        self.text.split_at(self.at)
    }

    /// Replace the text, with the cursor at the end.
    fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.at = self.text.len();
    }

    fn prev(&self) -> usize {
        self.text[..self.at].char_indices().next_back().map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text[self.at..]
            .chars()
            .next()
            .map_or(self.at, |c| self.at + c.len_utf8())
    }

    /// Apply a key; returns whether the text changed.
    fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Char(c) => {
                self.text.insert(self.at, c);
                self.at += c.len_utf8();
            }
            Key::Space => {
                self.text.insert(self.at, ' ');
                self.at += 1;
            }
            Key::Backspace if self.at > 0 => {
                let p = self.prev();
                self.text.replace_range(p..self.at, "");
                self.at = p;
            }
            Key::Delete | Key::Ctrl('d') if self.at < self.text.len() => {
                let n = self.next();
                self.text.replace_range(self.at..n, "");
            }
            Key::Ctrl('u') => {
                self.text.replace_range(..self.at, "");
                self.at = 0;
            }
            Key::Ctrl('w') => {
                let before = &self.text[..self.at];
                let start = before.trim_end().rfind(' ').map_or(0, |i| i + 1);
                self.text.replace_range(start..self.at, "");
                self.at = start;
            }
            Key::Left | Key::Ctrl('b') => {
                self.at = self.prev();
                return false;
            }
            Key::Right | Key::Ctrl('f') => {
                self.at = self.next();
                return false;
            }
            Key::Home | Key::Ctrl('a') => {
                self.at = 0;
                return false;
            }
            Key::End | Key::Ctrl('e') => {
                self.at = self.text.len();
                return false;
            }
            _ => return false,
        }
        true
    }
}

/// A line of text. An empty answer takes the default, shown dim until you type.
pub fn input(question: &str, default: Option<&str>) -> Result<String> {
    let mut q = Input::new(question);
    if let Some(d) = default {
        q = q.default(d);
    }
    q.ask()
}

/// A line of text shown as dots.
pub fn password(question: &str) -> Result<String> {
    Input::new(question).password().ask()
}

/// Make someone type `expected` exactly before going on: for things that can't be undone,
/// like deleting an app. Returns once they have; Esc or Ctrl-C is [`Error::Cancelled`].
///
/// ```no_run
/// tinsel::prompt::confirm_typing("Delete my-app and all its data?", "my-app")?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub fn confirm_typing(question: &str, expected: &str) -> Result<()> {
    Input::new(question)
        .placeholder(format!("type {expected} to confirm"))
        .validate(|s| {
            if s == expected {
                Ok(())
            } else {
                Err(format!("type “{expected}” exactly to go ahead"))
            }
        })
        .ask()
        .map(|_| ())
}

/// A number of any type that parses from text (`u16`, `f64`, …). Anything that doesn't parse
/// is refused and asked again.
///
/// ```no_run
/// let replicas: u32 = tinsel::prompt::number("Replicas", Some(3))?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub fn number<T>(question: &str, default: Option<T>) -> Result<T>
where
    T: FromStr + fmt::Display,
{
    let mut q = Input::new(question).validate(|s| {
        s.trim()
            .parse::<T>()
            .map(|_| ())
            .map_err(|_| format!("“{s}” isn't a {}", kind_of_number::<T>()))
    });
    if let Some(d) = default {
        q = q.default(d.to_string());
    }
    let answer = q.ask()?;
    answer.trim().parse().map_err(|_| Error::Invalid(answer))
}

/// "whole number" or "number", from the type's name.
fn kind_of_number<T>() -> &'static str {
    let name = std::any::type_name::<T>();
    if name.starts_with('u') {
        "whole number of 0 or more"
    } else if name.starts_with('i') {
        "whole number"
    } else {
        "number"
    }
}

// MARK: - Lists

const VISIBLE: usize = 8;

/// The slice of a long list to show, keeping the cursor in view.
fn window(len: usize, cursor: usize) -> std::ops::Range<usize> {
    let shown = len.min(VISIBLE);
    let top = cursor.saturating_sub(shown / 2).min(len - shown);
    top..top + shown
}

/// Whether `query`'s characters all appear in `option`, in order (ignoring case), and if so
/// where, so they can be highlighted.
fn fuzzy(option: &str, query: &str) -> Option<Vec<usize>> {
    let mut hits = Vec::new();
    let mut want = query.chars().flat_map(char::to_lowercase).peekable();
    for (i, c) in option.chars().enumerate() {
        let Some(&w) = want.peek() else { break };
        if c.to_lowercase().eq(std::iter::once(w)) {
            hits.push(i);
            want.next();
        }
    }
    want.peek().is_none().then_some(hits)
}

/// The options shown in a list question, narrowed by what's been typed in long lists.
struct Picker<'a> {
    options: &'a [&'a str],
    /// Long lists can be filtered by typing; short ones keep j/k for moving.
    filterable: bool,
    filter: String,
    /// Indices into `options` that match the filter, with the matched characters.
    shown: Vec<(usize, Vec<usize>)>,
    /// Position in `shown`.
    cursor: usize,
}

impl<'a> Picker<'a> {
    fn new(options: &'a [&'a str]) -> Self {
        let mut picker = Picker {
            options,
            filterable: options.len() > VISIBLE,
            filter: String::new(),
            shown: Vec::new(),
            cursor: 0,
        };
        picker.refilter();
        picker
    }

    fn refilter(&mut self) {
        let keep = self.shown.get(self.cursor).map(|(i, _)| *i);
        self.shown = (0..self.options.len())
            .filter_map(|i| fuzzy(self.options[i], &self.filter).map(|hits| (i, hits)))
            .collect();
        // stay on the same option if it's still there
        self.cursor = keep
            .and_then(|k| self.shown.iter().position(|(i, _)| *i == k))
            .unwrap_or(0);
    }

    fn current(&self) -> Option<usize> {
        self.shown.get(self.cursor).map(|(i, _)| *i)
    }

    /// Handle a key that moves or filters; returns `false` for keys it doesn't use.
    fn key(&mut self, key: Key) -> bool {
        let n = self.shown.len().max(1);
        match key {
            Key::Up => self.cursor = (self.cursor + n - 1) % n,
            Key::Down => self.cursor = (self.cursor + 1) % n,
            Key::Char('k') if !self.filterable => self.cursor = (self.cursor + n - 1) % n,
            Key::Char('j') if !self.filterable => self.cursor = (self.cursor + 1) % n,
            Key::Char(c) if self.filterable => {
                self.filter.push(c);
                self.refilter();
            }
            Key::Backspace if self.filterable => {
                self.filter.pop();
                self.refilter();
            }
            Key::Ctrl('u') if self.filterable => {
                self.filter.clear();
                self.refilter();
            }
            _ => return false,
        }
        true
    }

    /// The question line, with the filter typed so far.
    fn heading(&self, question: &str) -> String {
        if self.filter.is_empty() {
            ask(question)
        } else {
            format!("{}  {}{}", ask(question), paint(TEXT, &self.filter), paint(ROSE, "▏"))
        }
    }

    /// The visible options, each drawn by `row(option index, is cursor, label)`.
    fn rows(&self, row: impl Fn(usize, bool, String) -> String) -> Vec<String> {
        if self.shown.is_empty() {
            return vec![format!("      {}", paint(MUTED, "no matches"))];
        }
        window(self.shown.len(), self.cursor)
            .map(|pos| {
                let (i, hits) = &self.shown[pos];
                let here = pos == self.cursor;
                let base = if here { TEXT } else { SOFT };
                let label: String = self.options[*i]
                    .chars()
                    .enumerate()
                    .map(|(ci, c)| {
                        let color = if hits.contains(&ci) { ROSE } else { base };
                        paint(color, c.encode_utf8(&mut [0; 4]))
                    })
                    .collect();
                row(*i, here, label)
            })
            .collect()
    }
}

/// One of a list, moved through with ↑↓ (or j/k). Returns the chosen index. In lists longer
/// than eight, typing narrows the list to options containing those letters in order.
pub fn select(question: &str, options: &[&str]) -> Result<usize> {
    need_terminal()?;
    if options.is_empty() {
        return Err(Error::NoOptions);
    }
    let _raw = Raw::enter()?;
    let mut frame = Frame::default();
    let mut picker = Picker::new(options);
    let hint = if picker.filterable {
        "type to filter · ↑↓ move · enter choose"
    } else {
        "↑↓ move · enter choose"
    };
    loop {
        let mut lines = vec![picker.heading(question)];
        lines.extend(picker.rows(|_, here, label| {
            if here {
                format!("    {} {label}", paint(ROSE, "›"))
            } else {
                format!("      {label}")
            }
        }));
        lines.push(keys(hint));
        frame.draw(&lines);
        match read_key()? {
            Key::Enter => {
                if let Some(i) = picker.current() {
                    frame.draw(&[answered(question, options[i])]);
                    return Ok(i);
                }
            }
            // Esc clears a filter first, and only cancels once there's nothing to clear
            Key::Esc if !picker.filter.is_empty() => {
                picker.filter.clear();
                picker.refilter();
            }
            Key::Esc | Key::Interrupt => return Err(cancel(&mut frame, question)),
            key => {
                picker.key(key);
            }
        }
    }
}

/// Several of a list: space toggles, `a` toggles all, enter confirms. Returns the chosen
/// indices in order. In lists longer than eight, typing filters and Ctrl-A toggles all of
/// what's shown.
pub fn multiselect(question: &str, options: &[&str], preselected: &[usize]) -> Result<Vec<usize>> {
    need_terminal()?;
    if options.is_empty() {
        return Ok(Vec::new());
    }
    let _raw = Raw::enter()?;
    let mut frame = Frame::default();
    let mut picker = Picker::new(options);
    let mut picked: Vec<bool> = (0..options.len()).map(|i| preselected.contains(&i)).collect();
    let hint = if picker.filterable {
        "type to filter · ↑↓ move · space toggle · ctrl-a all · enter done"
    } else {
        "↑↓ move · space toggle · a all · enter done"
    };
    loop {
        let mut lines = vec![picker.heading(question)];
        lines.extend(picker.rows(|i, here, label| {
            let mark = if picked[i] {
                paint(ROSE, "◉")
            } else {
                paint(MUTED, "○")
            };
            let pointer = if here { paint(ROSE, "›") } else { " ".into() };
            format!("    {pointer} {mark} {label}")
        }));
        lines.push(keys(hint));
        frame.draw(&lines);
        match read_key()? {
            Key::Space => {
                if let Some(i) = picker.current() {
                    picked[i] = !picked[i];
                }
            }
            Key::Char('a') if !picker.filterable => {
                let all = picked.iter().all(|p| *p);
                picked.iter_mut().for_each(|p| *p = !all);
            }
            Key::Ctrl('a') => {
                let shown: Vec<usize> = picker.shown.iter().map(|(i, _)| *i).collect();
                let all = shown.iter().all(|&i| picked[i]);
                shown.iter().for_each(|&i| picked[i] = !all);
            }
            Key::Enter => break,
            Key::Esc if !picker.filter.is_empty() => {
                picker.filter.clear();
                picker.refilter();
            }
            Key::Esc | Key::Interrupt => return Err(cancel(&mut frame, question)),
            key => {
                picker.key(key);
            }
        }
    }
    let chosen: Vec<usize> = (0..options.len()).filter(|&i| picked[i]).collect();
    let names: Vec<&str> = chosen.iter().map(|&i| options[i]).collect();
    let summary = if names.is_empty() {
        "none".to_string()
    } else {
        text::truncate(&names.join(", "), 40)
    };
    frame.draw(&[answered(question, &summary)]);
    Ok(chosen)
}

// MARK: - Dates

//! Questions: yes/no, a line of text, a password, a number, a date, one of a list, several
//! of a list, or a longer answer in the user's editor.
//!
//! Drawn on stderr, so stdout stays clean for piping. Each question collapses to a single
//! `●  question  answer` line once answered.
//!
//! Every question returns a [`Result`], whose error says what happened in a way you can
//! match on: [`Error::Cancelled`] when someone presses Ctrl-C or Esc, [`Error::NotATerminal`]
//! when there's no one to ask. Without a terminal on stdin, `confirm` and the text questions
//! (`input`, `number`, `date`) read a line, so scripts can still drive them
//! (`echo yes | my-cli`); an answer read that way that doesn't make sense or fails validation
//! is an [`Error::Invalid`].
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
//!
//! Lists have builders too: [`Select`] and [`MultiSelect`] add groups under headings,
//! options that can't be picked (and why), a hint and validation, and [`Dropdown`] is for
//! long lists like versions, with notes and badges and room to type an answer of your own.
//!
//! # Testing a flow of questions
//!
//! [`script`] answers questions from a list of keys instead of the keyboard, on the current
//! thread, for as long as the [`Script`] it returns lives. Questions then draw nothing, and
//! [`Script::answered`] says what each one ended on, so a test can run the real code that
//! asks them.
//!
//! ```
//! use tinsel::app::Key;
//! use tinsel::prompt::{self, Select};
//!
//! let keys = prompt::script([Key::Down, Key::Enter]);
//! let picked = Select::new("Deploy to").options(["staging", "production"]).ask()?;
//! assert_eq!(picked, 1);
//! assert_eq!(keys.answered(), ["●  Deploy to  production"]);
//! # Ok::<(), prompt::Error>(())
//! ```
//!
//! # Going back
//!
//! For questions asked one after another, `.back()` lets ← go back: the question returns
//! [`Error::Back`] and you ask the one before it again. → always answers, as Enter does (in
//! typed answers, from the end of the line), so a flow can be stepped through with the arrows.
//!
//! ```no_run
//! use tinsel::prompt::{Error, Input, Select};
//!
//! let (mut name, mut target) = (String::new(), 0);
//! let mut step = 0;
//! while step < 2 {
//!     let asked = match step {
//!         0 => Input::new("Project name").ask().map(|n| name = n),
//!         _ => Select::new("Deploy to").options(["staging", "production"]).back().ask().map(|t| target = t),
//!     };
//!     match asked {
//!         Ok(()) => step += 1,
//!         Err(Error::Back) => step -= 1,
//!         Err(e) => return Err(e),
//!     }
//! }
//! # Ok::<(), Error>(())
//! ```

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::str::FromStr;

use crate::style::palette::{BLUSH, MUTED, RED, ROSE, SOFT, TEXT, TRACK};
use crate::style::{bold, italic, mix, paint, paint_bg, underline};
use crate::term::{RawMode, read_byte};
use crate::text;

/// Why a question didn't get an answer.
#[derive(Debug)]
pub enum Error {
    /// The person pressed Ctrl-C or Esc: they want to stop altogether. Exiting with status
    /// 130 is the convention.
    Cancelled,
    /// The person pressed ← to go back to the question before, to change their answer. Only
    /// questions asked with `.back()` (on [`Confirm`], [`Input`], [`Select`], [`MultiSelect`]
    /// and [`Dropdown`]) return it, so the caller can ask the previous question again; the
    /// question is cleared from the screen. Unlike [`Error::Cancelled`], the person wants to
    /// carry on.
    Back,
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
            Error::Back => f.write_str("went back to the previous question"),
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

/// The next key, from bytes that `read` gives one at a time (`None` after a pause).
fn read_key(read: &mut impl FnMut() -> io::Result<Option<u8>>) -> io::Result<Key> {
    let first = loop {
        // reads time out every 0.1s, which is what lets a lone Esc be told apart
        if let Some(b) = read()? {
            break b;
        }
    };
    Ok(match first {
        3 => Key::Interrupt,
        b'\r' | b'\n' => Key::Enter,
        b'\t' => Key::Tab,
        b' ' => Key::Space,
        127 | 8 => Key::Backspace,
        27 => match read()? {
            Some(b'[' | b'O') => match read()? {
                Some(b'A') => Key::Up,
                Some(b'B') => Key::Down,
                Some(b'C') => Key::Right,
                Some(b'D') => Key::Left,
                Some(b'H') => Key::Home,
                Some(b'F') => Key::End,
                // ESC [ n ~
                Some(n @ b'1'..=b'8') => {
                    let mut code = vec![n];
                    while let Some(b) = read()? {
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
                buf.extend(read()?);
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
    /// What was last drawn, kept for tests.
    lines: Vec<String>,
    /// Keep the lines without printing them.
    offscreen: bool,
}

impl Drop for Ui {
    fn drop(&mut self) {
        if self.record && !self.frame.lines.is_empty() {
            let lines = self.frame.lines.iter().map(|l| text::strip_ansi(l).trim().to_string());
            SCRIPT.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.answered.extend(lines);
                }
            });
        }
    }
}

impl Frame {
    fn draw(&mut self, lines: &[String]) {
        self.lines = lines.to_vec();
        if self.offscreen {
            return;
        }
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

/// Where a question reads keys and draws: the terminal in raw mode, or a script in tests.
struct Ui {
    keys: Box<dyn FnMut() -> io::Result<Key>>,
    frame: Frame,
    _raw: Option<Raw>,
    /// Hand the last lines to the running [`Script`] when done.
    record: bool,
}

impl Ui {
    /// The script's keys when one is running on this thread, otherwise the terminal.
    fn open() -> io::Result<Ui> {
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
        Ok(Ui {
            keys: Box::new(|| read_key(&mut read_byte)),
            frame: Frame::default(),
            _raw: Some(raw),
            record: false,
        })
    }

    /// Keys typed from a list, drawing nowhere; running out of keys is an error.
    #[cfg(test)]
    fn script(keys: &[Key]) -> Ui {
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

    fn key(&mut self) -> io::Result<Key> {
        (self.keys)()
    }

    fn draw(&mut self, lines: &[String]) {
        self.frame.draw(lines);
    }

    /// Clear the question away, for the one before it to be asked again.
    fn back(&mut self) -> Error {
        self.frame.draw(&[]);
        Error::Back
    }

    /// The last thing drawn, without colour.
    #[cfg(test)]
    fn shown(&self) -> Vec<String> {
        self.frame.lines.iter().map(|l| text::strip_ansi(l)).collect()
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

/// "  ← back", for the end of a question's line, when going back is on.
fn back_hint(back: bool) -> String {
    if back {
        format!("  {}", paint(MUTED, "← back"))
    } else {
        String::new()
    }
}

fn keys(hint: &str) -> String {
    format!("    {}", paint(MUTED, hint))
}

fn problem(why: &str) -> String {
    format!("     {} {}", paint(RED, "✕"), paint(mix(RED, SOFT, 0.3), why))
}

/// Whether there's someone to ask: a terminal on stdin, or a [`script`] of keys.
fn interactive() -> bool {
    scripted() || io::stdin().is_terminal()
}

fn need_terminal() -> Result<()> {
    if interactive() {
        Ok(())
    } else {
        Err(Error::NotATerminal)
    }
}

/// A line read from a pipe or file, for when there's no one to ask. The end of the input
/// reads as an empty line.
fn read_piped_line() -> Result<String> {
    Ok(read_piped()?.unwrap_or_default())
}

/// A line read from a pipe or file, or `None` at the end of the input.
fn read_piped() -> Result<Option<String>> {
    let mut s = String::new();
    if io::stdin().read_line(&mut s)? == 0 {
        return Ok(None);
    }
    Ok(Some(s.trim_end_matches(['\r', '\n']).to_string()))
}

// MARK: - Yes or no

/// Yes or no. Enter takes the default. [`Confirm`] has more options.
///
/// Without a terminal it reads a line instead: `y`, `yes`, `n` or `no` in any case, and an
/// empty line (or the end of the input) takes the default. Anything else is an
/// [`Error::Invalid`].
///
/// ```no_run
/// if tinsel::prompt::confirm("Deploy to production?", false)? {
///     tinsel::success("deploying");
/// }
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub fn confirm(question: &str, default: bool) -> Result<bool> {
    Confirm::new(question).default(default).ask()
}

/// A yes or no question with options. [`confirm`] is the shortcut. Answers no unless given
/// a [`default`](Confirm::default).
///
/// ```no_run
/// use tinsel::prompt::Confirm;
///
/// let tag = Confirm::new("Tag the release?").default(true).back().ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct Confirm {
    question: String,
    default: bool,
    back: bool,
    at_end: Option<bool>,
}

impl Confirm {
    /// A yes or no question.
    pub fn new(question: impl Into<String>) -> Self {
        Confirm {
            question: question.into(),
            default: false,
            back: false,
            at_end: None,
        }
    }

    /// The answer when Enter or → is pressed (or an empty line is piped in).
    pub fn default(mut self, yes: bool) -> Self {
        self.default = yes;
        self
    }

    /// Let ← go back to the question before, as [`Error::Back`].
    pub fn back(mut self) -> Self {
        self.back = true;
        self
    }

    /// The answer when there's no terminal and the piped input has already ended, so a
    /// script that runs out of answers can say no. The [`default`](Confirm::default) unless set.
    pub fn end_of_input(mut self, yes: bool) -> Self {
        self.at_end = Some(yes);
        self
    }

    /// Ask the question. Without a terminal, reads the answer from stdin as [`confirm`] does.
    pub fn ask(self) -> Result<bool> {
        if !interactive() {
            return match read_piped()? {
                Some(line) => yes_or_no(&line, self.default),
                None => Ok(self.at_end.unwrap_or(self.default)),
            };
        }
        self.run(&mut Ui::open()?)
    }

    fn run(&self, ui: &mut Ui) -> Result<bool> {
        let question = self.question.as_str();
        let choices = if self.default { "Y/n" } else { "y/N" };
        ui.draw(&[format!(
            "{}  {}{}",
            ask(question),
            paint(MUTED, choices),
            back_hint(self.back)
        )]);
        loop {
            let answer = match ui.key()? {
                Key::Char('y' | 'Y') => true,
                Key::Right => self.default,
                Key::Char('n' | 'N') => false,
                Key::Enter => self.default,
                Key::Left if self.back => return Err(ui.back()),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                _ => continue,
            };
            ui.draw(&[answered(question, if answer { "yes" } else { "no" })]);
            return Ok(answer);
        }
    }
}

/// A yes or no answer read from a pipe.
fn yes_or_no(line: &str, default: bool) -> Result<bool> {
    match line.trim().to_lowercase().as_str() {
        "" => Ok(default),
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(Error::Invalid(format!("“{}” isn't yes or no", line.trim()))),
    }
}

// MARK: - Text

/// What a validation check returns: why the answer won't do, if it won't.
type Checked = std::result::Result<(), String>;
type ManyCheck<'a> = Box<dyn Fn(&[usize]) -> Checked + 'a>;
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
    initial: Option<String>,
    placeholder: Option<String>,
    secret: bool,
    validate: Option<Validator<'a>>,
    complete: Option<Completer<'a>>,
    back: bool,
}

impl<'a> Input<'a> {
    /// A text question.
    pub fn new(question: impl Into<String>) -> Self {
        Input {
            question: question.into(),
            default: None,
            initial: None,
            placeholder: None,
            secret: false,
            validate: None,
            complete: None,
            back: false,
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

    /// Text already typed in when the question opens, with the cursor at its end, to edit or
    /// take as it is: the current value, say. Without a terminal, an empty line takes it.
    ///
    /// ```no_run
    /// let name = tinsel::prompt::Input::new("Name").initial("api-gateway").ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    pub fn initial(mut self, text: impl Into<String>) -> Self {
        self.initial = Some(text.into());
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

    /// Let ← go back to the question before, as [`Error::Back`], when the cursor is at the
    /// start of the line (so ← still moves through what's typed).
    ///
    /// ```no_run
    /// use tinsel::prompt::{Error, Input};
    ///
    /// match Input::new("Project name").back().ask() {
    ///     Ok(name) => println!("{name}"),
    ///     Err(Error::Back) => { /* ask the question before again */ }
    ///     Err(e) => return Err(e),
    /// }
    /// # Ok::<(), Error>(())
    /// ```
    pub fn back(mut self) -> Self {
        self.back = true;
        self
    }

    fn check(&self, value: &str) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(value))
    }

    /// What was typed, or the default when nothing was.
    fn resolve(&self, typed: String) -> String {
        if typed.is_empty() {
            self.default.clone().unwrap_or_default()
        } else {
            typed
        }
    }

    /// Ask the question.
    pub fn ask(self) -> Result<String> {
        if !interactive() {
            let line = read_piped_line()?;
            let value = match &self.initial {
                Some(initial) if line.is_empty() => initial.clone(),
                _ => self.resolve(line),
            };
            self.check(&value).map_err(Error::Invalid)?;
            return Ok(value);
        }
        self.run(&mut Ui::open()?)
    }

    fn run(&self, ui: &mut Ui) -> Result<String> {
        let mut edit = LineEdit::default();
        if let Some(initial) = &self.initial {
            edit.set(initial);
        }
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
            let mut lines = vec![format!(
                "{}  {shown}{}",
                ask(&self.question),
                back_hint(self.back && edit.at == 0)
            )];
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
            ui.draw(&lines);
            let key = match ui.key()? {
                // → answers from the end of the line, once there's no suggestion left to take
                Key::Right if edit.at == edit.text.len() && ghost.is_empty() => Key::Enter,
                key => key,
            };
            match key {
                Key::Enter => {
                    let value = self.resolve(edit.text.clone());
                    match self.check(&value) {
                        Ok(()) => {
                            let shown = if self.secret {
                                dots(value.chars().count())
                            } else {
                                value.clone()
                            };
                            ui.draw(&[answered(&self.question, &shown)]);
                            return Ok(value);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, &self.question)),
                Key::Left if self.back && edit.at == 0 => return Err(ui.back()),
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

/// How many rows a list shows at once, unless told otherwise.
const VISIBLE: usize = 8;

/// The slice of a long list to show, keeping the cursor in view.
fn window(len: usize, cursor: usize, rows: usize) -> std::ops::Range<usize> {
    let shown = len.min(rows);
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

/// One option in a list question.
#[derive(Default)]
struct Choice {
    label: String,
    /// A heading drawn above this option, starting a group.
    heading: Option<String>,
    /// Why it can't be picked.
    disabled: Option<String>,
    note: Option<String>,
    badges: Vec<String>,
    dim: bool,
}

/// What every list question has: the question, its options and how they're shown.
struct Choices {
    question: String,
    list: Vec<Choice>,
    /// A heading waiting for the next option.
    heading: Option<String>,
    hint: Option<String>,
    back: bool,
    rows: usize,
}

impl Choices {
    fn new(question: String) -> Self {
        Choices {
            question,
            list: Vec::new(),
            heading: None,
            hint: None,
            back: false,
            rows: VISIBLE,
        }
    }

    fn push(&mut self, label: String, disabled: Option<String>) {
        let heading = self.heading.take();
        self.list.push(Choice {
            label,
            heading,
            disabled,
            ..Choice::default()
        });
    }

    fn enabled(&self, i: usize) -> bool {
        self.list.get(i).is_some_and(|c| c.disabled.is_none())
    }

    /// The muted line of keys under the list.
    fn keys(&self, base: &str) -> String {
        keys(&format!("{base}{}", if self.back { " · ← back" } else { "" }))
    }
}

/// The builder methods every list question shares, for adding and decorating options.
macro_rules! list_builders {
    ($ty:ident) => {
        impl $ty<'_> {
            /// Add an option. Options are numbered from 0 in the order they're added,
            /// disabled ones included, and the answer is given as those numbers.
            pub fn option(mut self, label: impl Into<String>) -> Self {
                self.choices.push(label.into(), None);
                self
            }

            /// Add several options.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty),
                                "::new(\"Deploy to\").options([\"staging\", \"production\"]).ask()?;\n",
                                "# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            pub fn options<S: Into<String>>(mut self, labels: impl IntoIterator<Item = S>) -> Self {
                for label in labels {
                    self.choices.push(label.into(), None);
                }
                self
            }

            /// Add an option that can't be picked, drawn dim with the reason after it. The
            /// cursor skips over it, but it still takes up a number.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Deploy to\")\n",
                                "    .option(\"staging\")\n    .disabled(\"production\", \"frozen until Monday\")\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            pub fn disabled(mut self, label: impl Into<String>, reason: impl Into<String>) -> Self {
                self.choices.push(label.into(), Some(reason.into()));
                self
            }

            /// Start a group: a heading over the options added after it. It's not an option,
            /// so it can't be picked and takes up no number.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Service\")\n",
                                "    .heading(\"Frontend\")\n    .options([\"web\", \"docs\"])\n",
                                "    .heading(\"Backend\")\n    .options([\"api\", \"worker\"])\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            pub fn heading(mut self, title: impl Into<String>) -> Self {
                self.choices.heading = Some(title.into());
                self
            }

            /// Muted text lined up on the right of the option added last, such as when it
            /// was made.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Version\")\n",
                                "    .option(\"v1.2.0\").note(\"3 days ago\").badge(\"latest\")\n",
                                "    .option(\"v1.1.0\").note(\"2 weeks ago\")\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            pub fn note(mut self, text: impl Into<String>) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.note = Some(text.into());
                }
                self
            }

            /// A small badge after the option added last, such as `latest` or `current`.
            /// Call it again for another.
            pub fn badge(mut self, label: impl Into<String>) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.badges.push(label.into());
                }
                self
            }

            /// Draw the option added last quieter than the rest, for ones that are less
            /// likely to be wanted (a pre-release, say). It can still be picked.
            pub fn dim(mut self) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.dim = true;
                }
                self
            }

            /// A muted line under the question, to say more about what's being asked.
            pub fn hint(mut self, text: impl Into<String>) -> Self {
                self.choices.hint = Some(text.into());
                self
            }

            /// Let ← go back to the question before, as [`Error::Back`].
            pub fn back(mut self) -> Self {
                self.choices.back = true;
                self
            }

            /// How many rows to show at once (8 unless set); longer lists scroll, with a
            /// count of what's above and below.
            pub fn rows(mut self, rows: usize) -> Self {
                self.choices.rows = rows.max(1);
                self
            }
        }
    };
}

/// A row of a list as drawn.
enum Row {
    /// A group heading, over the option with this number.
    Heading(usize),
    /// An option, by position in what's shown.
    Option(usize),
    /// Use what's been typed as the answer.
    Custom,
}

/// The options shown in a list question, narrowed by what's been typed in long lists.
struct Picker<'a> {
    choices: &'a Choices,
    /// Lists that can be filtered by typing; others keep j/k for moving.
    filterable: bool,
    /// Whether what's typed can be the answer itself.
    custom: bool,
    filter: String,
    /// Indices into the options that match the filter, with the matched characters.
    shown: Vec<(usize, Vec<usize>)>,
    /// Position in `shown`, or `shown.len()` for the typed answer.
    cursor: usize,
}

impl<'a> Picker<'a> {
    fn new(choices: &'a Choices, filterable: bool, start: Option<usize>) -> Self {
        let mut picker = Picker {
            choices,
            filterable,
            custom: false,
            filter: String::new(),
            shown: Vec::new(),
            cursor: 0,
        };
        picker.refilter();
        if let Some(pos) = start.filter(|&i| choices.enabled(i)) {
            picker.cursor = pos;
        }
        picker
    }

    /// Whether the typed text is offered as an answer of its own.
    fn offers_custom(&self) -> bool {
        self.custom && !self.filter.is_empty() && !self.choices.list.iter().any(|c| c.label == self.filter)
    }

    /// How many positions the cursor can be in.
    fn len(&self) -> usize {
        self.shown.len() + self.offers_custom() as usize
    }

    fn selectable(&self, pos: usize) -> bool {
        match self.shown.get(pos) {
            Some((i, _)) => self.choices.enabled(*i),
            None => pos < self.len(),
        }
    }

    fn refilter(&mut self) {
        let keep = self.shown.get(self.cursor).map(|(i, _)| *i);
        self.shown = (0..self.choices.list.len())
            .filter_map(|i| fuzzy(&self.choices.list[i].label, &self.filter).map(|hits| (i, hits)))
            .collect();
        // stay on the same option if it's still there
        self.cursor = keep
            .and_then(|k| self.shown.iter().position(|(i, _)| *i == k))
            .filter(|&pos| self.selectable(pos))
            .or_else(|| (0..self.len()).find(|&pos| self.selectable(pos)))
            .unwrap_or(0);
    }

    fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.refilter();
    }

    fn clear_filter(&mut self) {
        self.filter.clear();
        self.refilter();
    }

    /// The option under the cursor, if it can be picked.
    fn current(&self) -> Option<usize> {
        self.shown
            .get(self.cursor)
            .map(|(i, _)| *i)
            .filter(|&i| self.choices.enabled(i))
    }

    /// Whether the cursor is on the typed answer.
    fn on_custom(&self) -> bool {
        self.offers_custom() && self.cursor == self.shown.len()
    }

    /// Move to the next option that can be picked, wrapping around.
    fn step(&mut self, down: bool) {
        let n = self.len();
        for k in 1..n {
            let pos = if down {
                (self.cursor + k) % n
            } else {
                (self.cursor + n - k) % n
            };
            if self.selectable(pos) {
                self.cursor = pos;
                return;
            }
        }
    }

    /// Move a page at a time, stopping at the ends.
    fn page(&mut self, down: bool) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let target = if down {
            (self.cursor + self.choices.rows).min(n - 1)
        } else {
            self.cursor.saturating_sub(self.choices.rows)
        };
        // the nearest option that can be picked, between the cursor and the target
        let found = if down {
            (self.cursor..=target).rev().find(|&pos| self.selectable(pos))
        } else {
            (target..=self.cursor).find(|&pos| self.selectable(pos))
        };
        if let Some(pos) = found {
            self.cursor = pos;
        }
    }

    /// Handle a key that moves or filters; returns `false` for keys it doesn't use.
    fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Up => self.step(false),
            Key::Down => self.step(true),
            Key::PageUp => self.page(false),
            Key::PageDown => self.page(true),
            Key::Char('k') if !self.filterable => self.step(false),
            Key::Char('j') if !self.filterable => self.step(true),
            Key::Char(c) if self.filterable => self.type_char(c),
            Key::Backspace if self.filterable => {
                self.filter.pop();
                self.refilter();
            }
            Key::Ctrl('u') if self.filterable => self.clear_filter(),
            _ => return false,
        }
        true
    }

    /// The question line, with the filter typed so far and how many options match it, then
    /// the hint, if there is one.
    fn top(&self) -> Vec<String> {
        let question = &self.choices.question;
        let mut lines = vec![if self.filter.is_empty() {
            ask(question)
        } else {
            format!(
                "{}  {}{}  {}",
                ask(question),
                paint(TEXT, &self.filter),
                paint(ROSE, "▏"),
                paint(MUTED, &format!("{} of {}", self.shown.len(), self.choices.list.len()))
            )
        }];
        lines.extend(self.choices.hint.as_deref().map(keys));
        lines
    }

    fn layout(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut after = 0;
        for (pos, (i, _)) in self.shown.iter().enumerate() {
            // only the nearest heading: groups in between have nothing shown
            if let Some(h) = (after..=*i).rev().find(|&h| self.choices.list[h].heading.is_some()) {
                rows.push(Row::Heading(h));
            }
            rows.push(Row::Option(pos));
            after = i + 1;
        }
        if self.offers_custom() {
            rows.push(Row::Custom);
        }
        rows
    }

    /// The visible rows, each option after `prefix(option, is cursor)`; with a count of
    /// options above and below when the list scrolls.
    fn rows(&self, prefix: impl Fn(Option<usize>, bool) -> String) -> Vec<String> {
        let layout = self.layout();
        if layout.is_empty() {
            return vec![format!("      {}", paint(MUTED, "no matches"))];
        }
        let at = layout
            .iter()
            .position(|r| match r {
                Row::Option(pos) => *pos == self.cursor,
                Row::Custom => self.on_custom(),
                Row::Heading(_) => false,
            })
            .unwrap_or(0);
        let range = window(layout.len(), at, self.choices.rows);
        let scrolls = layout.len() > self.choices.rows;
        let count = |rows: &[Row]| rows.iter().filter(|r| !matches!(r, Row::Heading(_))).count();
        let more = |n: usize, arrow: &str| {
            if n == 0 {
                String::new()
            } else {
                format!("      {}", paint(MUTED, &format!("{arrow} {n} more")))
            }
        };
        let (left, right) = self.widths();
        let mut lines = Vec::new();
        if scrolls {
            lines.push(more(count(&layout[..range.start]), "↑"));
        }
        for row in &layout[range.clone()] {
            lines.push(match row {
                Row::Heading(h) => {
                    let title = self.choices.list[*h].heading.as_deref().unwrap_or_default();
                    format!("    {}", bold(&paint(BLUSH, title)))
                }
                Row::Option(pos) => {
                    let (i, hits) = &self.shown[*pos];
                    let here = *pos == self.cursor;
                    format!("{}{}", prefix(Some(*i), here), self.body(*i, hits, here, left, right))
                }
                Row::Custom => {
                    let here = self.on_custom();
                    let typed = paint(if here { TEXT } else { SOFT }, &format!("“{}”", self.filter));
                    format!("{}{} {typed}", prefix(None, here), paint(MUTED, "use"))
                }
            });
        }
        if scrolls {
            lines.push(more(count(&layout[range.end..]), "↓"));
        }
        lines
    }

    /// How wide the label column (with badges and reasons) and the note column are, over
    /// every option so they don't shift as the list scrolls.
    fn widths(&self) -> (usize, usize) {
        let list = &self.choices.list;
        let left = list.iter().map(left_width).max().unwrap_or(0);
        let right = list
            .iter()
            .filter_map(|c| c.note.as_deref().map(text::width))
            .max()
            .unwrap_or(0);
        (left, right)
    }

    /// An option's label (matched letters picked out), badges, reason and note.
    fn body(&self, i: usize, hits: &[usize], here: bool, left: usize, right: usize) -> String {
        let c = &self.choices.list[i];
        let base = if c.disabled.is_some() || c.dim {
            MUTED
        } else if here {
            TEXT
        } else {
            SOFT
        };
        let mut s: String = c
            .label
            .chars()
            .enumerate()
            .map(|(ci, ch)| {
                let color = if hits.contains(&ci) && c.disabled.is_none() {
                    ROSE
                } else {
                    base
                };
                paint(color, ch.encode_utf8(&mut [0; 4]))
            })
            .collect();
        // the row under the cursor stands out as it does in app lists
        if here && base == TEXT {
            s = bold(&s);
        }
        for b in &c.badges {
            s += " ";
            s += &small_badge(b);
        }
        if let Some(why) = &c.disabled {
            s += "  ";
            s += &italic(&paint(MUTED, &format!("({why})")));
        }
        if let Some(note) = &c.note {
            s += &" ".repeat(left - left_width(c) + 2);
            s += &paint(MUTED, &text::pad_left(note, right));
        }
        s
    }
}

/// Columns taken by an option's label, badges and reason.
fn left_width(c: &Choice) -> usize {
    text::width(&c.label)
        + c.badges.iter().map(|b| text::width(b) + 3).sum::<usize>()
        + c.disabled.as_deref().map_or(0, |why| text::width(why) + 4)
}

/// A badge in the accent, or `[label]` without colour, where a pill wouldn't show.
fn small_badge(label: &str) -> String {
    if crate::style::color_mode() == crate::ColorMode::None {
        format!("[{label}]")
    } else {
        crate::badge(label, ROSE)
    }
}

/// The pointer column for lists that pick one.
fn pointer(here: bool) -> String {
    if here {
        format!("    {} ", paint(ROSE, "›"))
    } else {
        "      ".into()
    }
}

/// One of a list, moved through with ↑↓ (or j/k). Returns the chosen index. In lists longer
/// than eight, typing narrows the list to options containing those letters in order, and
/// Esc clears what's typed before it cancels. [`Select`] has more options.
pub fn select(question: &str, options: &[&str]) -> Result<usize> {
    Select::new(question).options(options.iter().copied()).ask()
}

/// One of a list, with options: a default, groups under headings, options that can't be
/// picked (and why), notes and badges, a hint, validation and going back. [`select`] is the
/// shortcut. Returns the number of the chosen option, counting from 0 in the order they
/// were added.
///
/// ↑↓ (or j/k) move, PgUp/PgDn move a page. Lists longer than [`rows`](Select::rows) filter
/// as you type, and Esc clears the filter before it cancels.
///
/// Without a terminal, the [`default`](Select::default) is the answer, if there is one;
/// otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::Select;
///
/// let target = Select::new("Deploy to")
///     .hint("production needs a second approval")
///     .heading("Shared")
///     .option("staging")
///     .option("production")
///     .disabled("legacy", "retired")
///     .heading("Yours")
///     .option("preview")
///     .default(0)
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct Select<'a> {
    choices: Choices,
    default: Option<usize>,
    validate: Option<Box<dyn Fn(usize) -> Checked + 'a>>,
}

list_builders!(Select);

impl<'a> Select<'a> {
    /// A question with one answer from a list.
    pub fn new(question: impl Into<String>) -> Self {
        Select {
            choices: Choices::new(question.into()),
            default: None,
            validate: None,
        }
    }

    /// Start on this option (by number), and answer with it when there's no terminal.
    pub fn default(mut self, index: usize) -> Self {
        self.default = Some(index);
        self
    }

    /// Check the choice when Enter is pressed. An `Err` is shown under the question, which
    /// stays open.
    ///
    /// ```no_run
    /// let size = tinsel::prompt::Select::new("Size")
    ///     .options(["small", "large"])
    ///     .validate(|i| if i == 1 { Err("large needs a paid plan".into()) } else { Ok(()) })
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    pub fn validate(mut self, check: impl Fn(usize) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, i: usize) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(i))
    }

    /// Ask the question.
    pub fn ask(self) -> Result<usize> {
        if !interactive() {
            return self.unasked();
        }
        if !(0..self.choices.list.len()).any(|i| self.choices.enabled(i)) {
            return Err(Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if there is one.
    fn unasked(&self) -> Result<usize> {
        let i = self
            .default
            .filter(|&i| self.choices.enabled(i))
            .ok_or(Error::NotATerminal)?;
        self.check(i).map_err(Error::Invalid)?;
        Ok(i)
    }

    fn run(&self, ui: &mut Ui) -> Result<usize> {
        let question = &self.choices.question;
        let filterable = self.choices.list.len() > self.choices.rows;
        let mut picker = Picker::new(&self.choices, filterable, self.default);
        let hint = if filterable {
            "type to filter · ↑↓ move · enter choose"
        } else {
            "↑↓ move · enter choose"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(|_, here| pointer(here)));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            match ui.key()? {
                Key::Enter | Key::Right => {
                    if let Some(i) = picker.current() {
                        match self.check(i) {
                            Ok(()) => {
                                ui.draw(&[answered(question, &self.choices.list[i].label)]);
                                return Ok(i);
                            }
                            Err(why) => trouble = Some(why),
                        }
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                // Esc clears a filter first, and only cancels once there's nothing to clear
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    if picker.key(key) {
                        trouble = None;
                    }
                }
            }
        }
    }
}

/// Several of a list: space toggles, `a` toggles all, enter confirms. Returns the chosen
/// indices in order. In lists longer than eight, typing filters and Ctrl-A toggles all of
/// what's shown. [`MultiSelect`] has more options.
pub fn multiselect(question: &str, options: &[&str], preselected: &[usize]) -> Result<Vec<usize>> {
    need_terminal()?;
    MultiSelect::new(question)
        .options(options.iter().copied())
        .default(preselected)
        .ask()
}

/// Several of a list, with options: some ticked to start with, groups under headings,
/// options that can't be picked (and why), notes and badges, a hint, validation and going
/// back. [`multiselect`] is the shortcut. Returns the numbers of the chosen options in
/// order, counting from 0 in the order they were added.
///
/// Space toggles, `a` toggles all (Ctrl-A, all of what's shown, in lists that filter as you
/// type), Enter is done.
///
/// Without a terminal, the [`default`](MultiSelect::default) is the answer, if one was
/// given; otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::MultiSelect;
///
/// let services = MultiSelect::new("Restart")
///     .options(["api", "web", "worker"])
///     .default(&[0])
///     .validate(|picked| if picked.is_empty() { Err("pick at least one".into()) } else { Ok(()) })
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct MultiSelect<'a> {
    choices: Choices,
    default: Option<Vec<usize>>,
    validate: Option<ManyCheck<'a>>,
}

list_builders!(MultiSelect);

impl<'a> MultiSelect<'a> {
    /// A question with any number of answers from a list.
    pub fn new(question: impl Into<String>) -> Self {
        MultiSelect {
            choices: Choices::new(question.into()),
            default: None,
            validate: None,
        }
    }

    /// Tick these options (by number) to start with, and answer with them when there's no
    /// terminal. Options that can't be picked stay unticked.
    pub fn default(mut self, indices: &[usize]) -> Self {
        self.default = Some(indices.to_vec());
        self
    }

    /// Check the choices when Enter is pressed. An `Err` is shown under the question, which
    /// stays open.
    pub fn validate(mut self, check: impl Fn(&[usize]) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, picked: &[usize]) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(picked))
    }

    fn ticked(&self) -> Vec<bool> {
        let default = self.default.as_deref().unwrap_or_default();
        (0..self.choices.list.len())
            .map(|i| default.contains(&i) && self.choices.enabled(i))
            .collect()
    }

    /// Ask the question.
    pub fn ask(self) -> Result<Vec<usize>> {
        if !interactive() {
            return self.unasked();
        }
        if self.choices.list.is_empty() {
            return self.check(&[]).map(|()| Vec::new()).map_err(|_| Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if one was given.
    fn unasked(&self) -> Result<Vec<usize>> {
        if self.default.is_none() {
            return Err(Error::NotATerminal);
        }
        let ticked = self.ticked();
        let chosen: Vec<usize> = (0..ticked.len()).filter(|&i| ticked[i]).collect();
        self.check(&chosen).map_err(Error::Invalid)?;
        Ok(chosen)
    }

    fn run(&self, ui: &mut Ui) -> Result<Vec<usize>> {
        let question = &self.choices.question;
        let filterable = self.choices.list.len() > self.choices.rows;
        let mut picker = Picker::new(&self.choices, filterable, None);
        let mut picked = self.ticked();
        let hint = if filterable {
            "type to filter · ↑↓ move · space toggle · ctrl-a all · enter done"
        } else {
            "↑↓ move · space toggle · a all · enter done"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(|i, here| {
                let i = i.unwrap_or_default();
                let mark = if picked[i] {
                    paint(ROSE, "◉")
                } else if self.choices.enabled(i) {
                    paint(MUTED, "○")
                } else {
                    paint(MUTED, "–")
                };
                let pointer = if here { paint(ROSE, "›") } else { " ".into() };
                format!("    {pointer} {mark} ")
            }));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            let toggle_all = |picked: &mut Vec<bool>, among: Vec<usize>| {
                let among: Vec<usize> = among.into_iter().filter(|&i| self.choices.enabled(i)).collect();
                let all = among.iter().all(|&i| picked[i]);
                among.iter().for_each(|&i| picked[i] = !all);
            };
            match ui.key()? {
                Key::Space => {
                    if let Some(i) = picker.current() {
                        picked[i] = !picked[i];
                    }
                    trouble = None;
                }
                Key::Char('a') if !picker.filterable => {
                    toggle_all(&mut picked, (0..self.choices.list.len()).collect());
                    trouble = None;
                }
                Key::Ctrl('a') => {
                    toggle_all(&mut picked, picker.shown.iter().map(|(i, _)| *i).collect());
                    trouble = None;
                }
                Key::Enter | Key::Right => {
                    let chosen: Vec<usize> = (0..picked.len()).filter(|&i| picked[i]).collect();
                    match self.check(&chosen) {
                        Ok(()) => {
                            let names: Vec<&str> =
                                chosen.iter().map(|&i| self.choices.list[i].label.as_str()).collect();
                            ui.draw(&[answered(question, &summary(&names))]);
                            return Ok(chosen);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    picker.key(key);
                }
            }
        }
    }
}

/// The chosen names for an answered line: as many whole names as fit, then how many more.
fn summary(names: &[&str]) -> String {
    let Some(first) = names.first() else {
        return "none".into();
    };
    let mut shown = text::truncate(first, 40);
    for (k, name) in names.iter().enumerate().skip(1) {
        let rest = names.len() - k - 1;
        let tail = if rest > 0 { format!(" and {rest} more").len() } else { 0 };
        if text::width(&shown) + 2 + text::width(name) + tail > 40 {
            return format!("{shown} and {} more", names.len() - k);
        }
        shown = format!("{shown}, {name}");
    }
    shown
}

/// One of a long list, such as versions or branches, as a dropdown: a window of rows that
/// scrolls, filtering as you type, with notes lined up on the right, badges, and options
/// drawn quieter than the rest. With [`allow_custom`](Dropdown::allow_custom), what's typed
/// can be the answer too. Returns the chosen option's label, or the typed answer.
///
/// Without a terminal, the [`default`](Dropdown::default) is the answer, if there is one;
/// otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::Dropdown;
///
/// let version = Dropdown::new("Roll back to")
///     .option("v1.2.0").note("3 days ago").badge("latest").badge("current")
///     .option("v1.2.0-rc.1").note("5 days ago").dim()
///     .option("v1.1.0").note("2 weeks ago")
///     .option("v1.0.0").note("2 months ago")
///     .default(2)
///     .allow_custom()
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct Dropdown<'a> {
    choices: Choices,
    default: Option<usize>,
    custom: bool,
    validate: Option<Validator<'a>>,
}

list_builders!(Dropdown);

impl<'a> Dropdown<'a> {
    /// A question with one answer from a long list.
    pub fn new(question: impl Into<String>) -> Self {
        Dropdown {
            choices: Choices::new(question.into()),
            default: None,
            custom: false,
            validate: None,
        }
    }

    /// Start on this option (by number), and answer with it when there's no terminal.
    pub fn default(mut self, index: usize) -> Self {
        self.default = Some(index);
        self
    }

    /// Let the answer be something that isn't in the list: once something's typed that
    /// isn't an option, it's offered at the end of the list to choose.
    pub fn allow_custom(mut self) -> Self {
        self.custom = true;
        self
    }

    /// Check the answer when Enter is pressed, whether an option or typed. An `Err` is
    /// shown under the question, which stays open.
    ///
    /// ```no_run
    /// let tag = tinsel::prompt::Dropdown::new("Tag")
    ///     .options(["v1.1.0", "v1.0.0"])
    ///     .allow_custom()
    ///     .validate(|s| if s.starts_with('v') { Ok(()) } else { Err("tags start with v".into()) })
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    pub fn validate(mut self, check: impl Fn(&str) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, value: &str) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(value))
    }

    /// Ask the question.
    pub fn ask(self) -> Result<String> {
        if !interactive() {
            return self.unasked();
        }
        if !self.custom && !(0..self.choices.list.len()).any(|i| self.choices.enabled(i)) {
            return Err(Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if there is one.
    fn unasked(&self) -> Result<String> {
        let i = self
            .default
            .filter(|&i| self.choices.enabled(i))
            .ok_or(Error::NotATerminal)?;
        let value = self.choices.list[i].label.clone();
        self.check(&value).map_err(Error::Invalid)?;
        Ok(value)
    }

    fn run(&self, ui: &mut Ui) -> Result<String> {
        let question = &self.choices.question;
        let mut picker = Picker::new(&self.choices, true, self.default);
        picker.custom = self.custom;
        let hint = if self.custom {
            "type to filter or enter your own · ↑↓ move · enter choose"
        } else {
            "type to filter · ↑↓ move · enter choose"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(|_, here| pointer(here)));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            match ui.key()? {
                Key::Enter | Key::Right => {
                    let value = match picker.current() {
                        Some(i) => self.choices.list[i].label.clone(),
                        None if picker.on_custom() => picker.filter.clone(),
                        None => continue,
                    };
                    match self.check(&value) {
                        Ok(()) => {
                            ui.draw(&[answered(question, &value)]);
                            return Ok(value);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                Key::Space => {
                    picker.type_char(' ');
                    trouble = None;
                }
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    if picker.key(key) {
                        trouble = None;
                    }
                }
            }
        }
    }
}

// MARK: - Dates

/// A calendar date, as returned by [`date`]. Shown and parsed as `YYYY-MM-DD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// The year, e.g. 2026.
    pub year: i32,
    /// The month, 1-12.
    pub month: u8,
    /// The day of the month, from 1.
    pub day: u8,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl Date {
    /// A date, if it exists (no February 30th).
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)).then_some(Date {
            year,
            month,
            day,
        })
    }

    /// Today, in the local time zone where that can be found (Unix), otherwise in UTC.
    pub fn today() -> Date {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Date::from_days((secs + crate::term::utc_offset_secs(secs)).div_euclid(86_400))
    }

    /// Days since 1970-01-01.
    fn days(self) -> i64 {
        // Howard Hinnant's days_from_civil
        let y = self.year as i64 - (self.month <= 2) as i64;
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn from_days(z: i64) -> Date {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        Date {
            year: (yoe + era * 400 + (month <= 2) as i64) as i32,
            month,
            day,
        }
    }

    fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }

    /// The same day `n` months on, or the month's last day if it's shorter.
    fn add_months(self, n: i32) -> Date {
        let m = self.year * 12 + self.month as i32 - 1 + n;
        let (year, month) = (m.div_euclid(12), (m.rem_euclid(12) + 1) as u8);
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }

    /// 0 for Monday through 6 for Sunday.
    fn weekday(self) -> usize {
        // 1970-01-01 was a Thursday
        (self.days() + 3).rem_euclid(7) as usize
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for Date {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Date, String> {
        let bad = || format!("“{s}” isn't a date like 2026-03-14");
        let mut parts = s.trim().splitn(3, '-');
        let mut next = || parts.next().and_then(|p| p.parse::<i32>().ok()).ok_or_else(bad);
        let (y, m, d) = (next()?, next()?, next()?);
        let (m, d) = (u8::try_from(m).map_err(|_| bad())?, u8::try_from(d).map_err(|_| bad())?);
        Date::new(y, m, d).ok_or_else(bad)
    }
}

/// A date picked from a calendar: ←→ move a day, ↑↓ a week, PgUp/PgDn (or `[` `]`) a month,
/// `t` jumps to today. Starts on `default`, or today.
///
/// ```no_run
/// let when = tinsel::prompt::date("Release on", None)?;
/// println!("{when}");   // 2026-11-12
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub fn date(question: &str, default: Option<Date>) -> Result<Date> {
    let today = Date::today();
    let start = default.unwrap_or(today);
    if !interactive() {
        let line = read_piped_line()?;
        if line.trim().is_empty() {
            return Ok(start);
        }
        return line.parse().map_err(Error::Invalid);
    }
    let mut ui = Ui::open()?;
    let mut at = start;
    loop {
        let mut lines = vec![format!("{}  {}", ask(question), paint(BLUSH, &at.to_string()))];
        lines.extend(calendar(at, today));
        lines.push(keys("←→ day · ↑↓ week · pgup/pgdn month · t today · enter choose"));
        ui.draw(&lines);
        at = match ui.key()? {
            Key::Left | Key::Char('h') => at.add_days(-1),
            Key::Right | Key::Char('l') => at.add_days(1),
            Key::Up | Key::Char('k') => at.add_days(-7),
            Key::Down | Key::Char('j') => at.add_days(7),
            Key::PageUp | Key::Char('[') => at.add_months(-1),
            Key::PageDown | Key::Char(']') => at.add_months(1),
            Key::Char('t') => today,
            Key::Enter => break,
            Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
            _ => at,
        };
    }
    ui.draw(&[answered(question, &at.to_string())]);
    Ok(at)
}

/// A month laid out Monday to Sunday, the chosen day as a pill and today underlined.
fn calendar(at: Date, today: Date) -> Vec<String> {
    let first = Date { day: 1, ..at };
    let title = format!("{} {}", MONTHS[at.month as usize - 1], at.year);
    let mut lines = vec![
        format!("       {}", paint(TEXT, &title)),
        format!("    {}", paint(MUTED, " Mo  Tu  We  Th  Fr  Sa  Su")),
    ];
    let mut row = "    ".to_string() + &"    ".repeat(first.weekday());
    for day in 1..=days_in_month(at.year, at.month) {
        let d = Date { day, ..at };
        let num = format!("{day:>2}");
        row += &if d == at {
            paint_bg(ROSE, mix(TRACK, ROSE, 0.28), &format!(" {num} "))
        } else if d == today {
            format!(" {} ", underline(&paint(ROSE, &num)))
        } else {
            format!(" {} ", paint(SOFT, &num))
        };
        if d.weekday() == 6 {
            lines.push(std::mem::replace(&mut row, "    ".into()));
        }
    }
    if row.trim() != "" {
        lines.push(row);
    }
    lines
}

// MARK: - Editor

/// A longer answer written in the user's editor (`$VISUAL`, then `$EDITOR`, then `vi`, or
/// Notepad on Windows), starting from `initial`. Returns what was saved. For commit messages,
/// release notes, anything more than a line.
///
/// ```no_run
/// let notes = tinsel::prompt::editor("Release notes", "## What's new\n\n")?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub fn editor(question: &str, initial: &str) -> Result<String> {
    need_terminal()?;
    let command = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| if cfg!(windows) { "notepad" } else { "vi" }.into());
    let mut words = command.split_whitespace();
    let program = words.next().unwrap_or("vi");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let path = std::env::temp_dir().join(format!("tinsel-{}-{nanos}.md", std::process::id()));
    std::fs::write(&path, initial)?;

    let mut frame = Frame::default();
    frame.draw(&[format!(
        "{}  {}",
        ask(question),
        paint(MUTED, &format!("opening {program}…"))
    )]);
    let status = std::process::Command::new(program).args(words).arg(&path).status();
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    match status {
        Ok(s) if s.success() => {}
        Ok(_) => return Err(cancel(&mut frame, question)),
        Err(e) => {
            frame.draw(&[format!(
                "{}  {}",
                ask(question),
                paint(RED, &format!("couldn't open {program}"))
            )]);
            return Err(Error::Io(e));
        }
    }
    let text = text?;
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    frame.draw(&[answered(question, &crate::format::plural(lines as u64, "line"))]);
    Ok(text)
}

// MARK: - Scripts, for tests

struct Scripted {
    keys: std::collections::VecDeque<crate::app::Key>,
    answered: Vec<String>,
}

thread_local! {
    static SCRIPT: std::cell::RefCell<Option<Scripted>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn scripted() -> bool {
    SCRIPT.with(|s| s.borrow().is_some())
}

/// The running script's next key, if there's a script and it has one left.
pub(crate) fn next_scripted() -> Option<crate::app::Key> {
    SCRIPT.with(|s| s.borrow_mut().as_mut().and_then(|s| s.keys.pop_front()))
}

impl From<crate::app::Key> for Key {
    fn from(key: crate::app::Key) -> Key {
        use crate::app::Key as K;
        match key {
            K::Char(' ') => Key::Space,
            K::Char(c) => Key::Char(c),
            K::Ctrl('c') => Key::Interrupt,
            K::Ctrl(c) => Key::Ctrl(c),
            K::Enter => Key::Enter,
            K::Tab => Key::Tab,
            K::Backspace => Key::Backspace,
            K::Delete => Key::Delete,
            K::Esc => Key::Esc,
            K::Up => Key::Up,
            K::Down => Key::Down,
            K::Left => Key::Left,
            K::Right => Key::Right,
            K::Home => Key::Home,
            K::End => Key::End,
            K::PageUp => Key::PageUp,
            K::PageDown => Key::PageDown,
            K::BackTab | K::F(_) => Key::Other,
        }
    }
}

/// Answer the questions asked on this thread from `keys` instead of the keyboard, until the
/// returned [`Script`] is dropped. For tests of code that asks questions.
///
/// Questions behave as they do in a terminal (a piped stdin is ignored) but draw nothing.
/// A question that needs a key after the last one fails with an [`Error::Io`] of kind
/// `UnexpectedEof`. [`editor`] still needs a terminal. [`typed`] turns text into keys.
///
/// ```
/// use tinsel::app::Key;
/// use tinsel::prompt;
///
/// let mut keys = prompt::typed("v2");
/// keys.push(Key::Enter);
/// let _script = prompt::script(keys);
/// assert_eq!(prompt::input("Version", None)?, "v2");
/// # Ok::<(), prompt::Error>(())
/// ```
pub fn script(keys: impl IntoIterator<Item = crate::app::Key>) -> Script {
    let keys = keys.into_iter().collect();
    SCRIPT.with(|s| {
        *s.borrow_mut() = Some(Scripted {
            keys,
            answered: Vec::new(),
        })
    });
    Script {
        _thread: std::marker::PhantomData,
    }
}

/// The keys to type `text`, one per character.
pub fn typed(text: &str) -> Vec<crate::app::Key> {
    text.chars().map(crate::app::Key::Char).collect()
}

/// Questions on this thread answered from a list of keys; made by [`script`]. Dropping it
/// gives the keyboard back.
#[must_use = "the script stops when this is dropped"]
pub struct Script {
    // tied to the thread whose questions it answers
    _thread: std::marker::PhantomData<*const ()>,
}

impl Script {
    /// What each question showed when it was done, without colour or indent, in the order
    /// they were asked: `●  Question  answer` for an answer, `✕  Question  cancelled`
    /// for Esc or Ctrl-C. A question left with ← shows nothing, so it isn't listed.
    pub fn answered(&self) -> Vec<String> {
        SCRIPT.with(|s| s.borrow().as_ref().map(|s| s.answered.clone()).unwrap_or_default())
    }

    /// How many keys haven't been read yet.
    pub fn remaining(&self) -> usize {
        SCRIPT.with(|s| s.borrow().as_ref().map_or(0, |s| s.keys.len()))
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        SCRIPT.with(|s| *s.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_answers_a_flow_of_questions_and_goes_back() {
        use crate::app::Key as K;
        let mut keys = vec![K::Down, K::Enter, K::Left, K::Enter, K::Char(' '), K::Enter];
        keys.extend(crate::prompt::typed("v2"));
        keys.extend([K::Enter, K::Char('y')]);
        let script = script(keys);
        let env = Select::new("Env").options(["dev", "sit"]).ask().unwrap();
        assert_eq!(env, 1);
        assert!(matches!(
            MultiSelect::new("Parts").options(["a", "b"]).back().ask(),
            Err(Error::Back)
        ));
        assert_eq!(
            Select::new("Env").options(["dev", "sit"]).default(env).ask().unwrap(),
            1
        );
        assert_eq!(MultiSelect::new("Parts").options(["a", "b"]).ask().unwrap(), [0]);
        assert_eq!(input("Version", None).unwrap(), "v2");
        assert!(confirm("Go?", false).unwrap());
        assert_eq!(script.remaining(), 0);
        assert_eq!(
            script.answered(),
            [
                "●  Env  sit",
                "●  Env  sit",
                "●  Parts  a",
                "●  Version  v2",
                "●  Go?  yes"
            ]
        );
        assert!(matches!(confirm("More?", false), Err(Error::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof));
        drop(script);
        assert!(!scripted());
    }

    #[test]
    fn ctrl_c_in_a_script_cancels() {
        let script = script([crate::app::Key::Ctrl('c')]);
        assert!(matches!(confirm("Go?", true), Err(Error::Cancelled)));
        assert_eq!(script.answered(), ["✕  Go?  cancelled"]);
    }

    #[test]
    fn fuzzy_matches_letters_in_order() {
        assert_eq!(fuzzy("production", "pdn"), Some(vec![0, 3, 9]));
        assert_eq!(fuzzy("Staging", "stg"), Some(vec![0, 1, 3]));
        assert_eq!(fuzzy("staging", "gts"), None);
        assert_eq!(fuzzy("anything", ""), Some(vec![]));
    }

    #[test]
    fn dates_round_trip_through_days() {
        for d in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 11, 12),
            (1969, 12, 31),
            (2400, 12, 31),
        ] {
            let date = Date::new(d.0, d.1, d.2).unwrap();
            assert_eq!(Date::from_days(date.days()), date);
        }
        assert_eq!(Date::new(1970, 1, 1).unwrap().days(), 0);
        // 2026-11-12 is a Thursday
        assert_eq!(Date::new(2026, 11, 12).unwrap().weekday(), 3);
    }

    #[test]
    fn months_clamp_to_their_length() {
        let jan31 = Date::new(2027, 1, 31).unwrap();
        assert_eq!(jan31.add_months(1), Date::new(2027, 2, 28).unwrap());
        assert_eq!(jan31.add_months(-2), Date::new(2026, 11, 30).unwrap());
        assert_eq!(Date::new(2026, 2, 29), None);
    }

    #[test]
    fn dates_parse() {
        assert_eq!("2026-03-14".parse(), Ok(Date::new(2026, 3, 14).unwrap()));
        assert!("2026-13-01".parse::<Date>().is_err());
        assert!("soon".parse::<Date>().is_err());
    }

    #[test]
    fn line_editing_moves_and_deletes() {
        let mut e = LineEdit::default();
        for c in "deploy now".chars() {
            e.key(if c == ' ' { Key::Space } else { Key::Char(c) });
        }
        e.key(Key::Ctrl('w'));
        assert_eq!(e.text, "deploy ");
        e.key(Key::Home);
        e.key(Key::Char('>'));
        assert_eq!(e.split(), (">", "deploy "));
        e.key(Key::Right);
        e.key(Key::Backspace);
        assert_eq!(e.text, ">eploy ");
        e.key(Key::End);
        e.key(Key::Ctrl('u'));
        assert_eq!(e.text, "");
    }

    fn keys_of(bytes: &[u8]) -> Vec<Key> {
        let at = std::cell::Cell::new(0);
        let mut read = || {
            at.set(at.get() + 1);
            Ok(bytes.get(at.get() - 1).copied())
        };
        let mut keys = Vec::new();
        while at.get() < bytes.len() {
            keys.push(read_key(&mut read).unwrap());
        }
        keys
    }

    #[test]
    fn bytes_become_keys() {
        assert_eq!(keys_of(b"\x1b[D"), [Key::Left]);
        assert_eq!(keys_of(b"\x1b[5~a\r"), [Key::PageUp, Key::Char('a'), Key::Enter]);
        assert_eq!(keys_of("é\x03".as_bytes()), [Key::Char('é'), Key::Interrupt]);
        assert_eq!(keys_of(b"\x1b"), [Key::Esc]);
    }

    fn typed(s: &str) -> Vec<Key> {
        s.chars()
            .map(|c| if c == ' ' { Key::Space } else { Key::Char(c) })
            .collect()
    }

    #[test]
    fn right_answers_like_enter() {
        assert!(
            Confirm::new("Go?")
                .default(true)
                .run(&mut Ui::script(&[Key::Right]))
                .unwrap()
        );
        let env = Select::new("Env").options(["dev", "sit"]);
        assert_eq!(env.run(&mut Ui::script(&[Key::Down, Key::Right])).unwrap(), 1);
        let parts = MultiSelect::new("Parts").options(["a", "b"]);
        assert_eq!(parts.run(&mut Ui::script(&[Key::Space, Key::Right])).unwrap(), [0]);
        // in typed text → moves first, and answers only from the end of the line
        let mut keys = typed("v2");
        keys.extend([Key::Home, Key::Right, Key::Char('.'), Key::Right, Key::Right]);
        assert_eq!(Input::new("Version").run(&mut Ui::script(&keys)).unwrap(), "v.2");
        // with a suggestion showing, → takes it, and the next → answers
        let region = Input::new("Region").suggestions(&["eu-west-1"]);
        let mut keys = typed("eu");
        keys.extend([Key::Right, Key::Right]);
        assert_eq!(region.run(&mut Ui::script(&keys)).unwrap(), "eu-west-1");
    }

    #[test]
    fn initial_text_can_be_taken_or_edited() {
        let name = Input::new("Name").initial("api").default("web");
        let mut ui = Ui::script(&[Key::Enter]);
        assert_eq!(name.run(&mut ui).unwrap(), "api");
        assert_eq!(ui.shown(), ["  ●  Name  api"]);
        let keys = [Key::Backspace, Key::Char('p'), Key::Enter];
        assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "app");
        // cleared, the default is the answer as usual
        assert_eq!(name.run(&mut Ui::script(&[Key::Ctrl('u'), Key::Enter])).unwrap(), "web");
    }

    #[test]
    fn confirm_takes_keys_and_the_default() {
        let go = Confirm::new("Go?").default(true);
        assert!(!go.run(&mut Ui::script(&[Key::Char('N')])).unwrap());
        let mut ui = Ui::script(&[Key::Down, Key::Enter]);
        assert!(go.run(&mut ui).unwrap());
        assert_eq!(ui.shown(), ["  ●  Go?  yes"]);
        assert!(matches!(go.run(&mut Ui::script(&[Key::Esc])), Err(Error::Cancelled)));
    }

    #[test]
    fn input_edits_and_validates() {
        let q = Input::new("Name").validate(|s| if s.len() > 2 { Ok(()) } else { Err("too short".into()) });
        let mut keys = typed("ab");
        keys.push(Key::Enter);
        let mut ui = Ui::script(&keys);
        assert!(q.run(&mut ui).is_err());
        assert_eq!(ui.shown()[1], "     ✕ too short");
        keys.extend([Key::Home, Key::Char('>'), Key::Enter]);
        let mut ui = Ui::script(&keys);
        assert_eq!(q.run(&mut ui).unwrap(), ">ab");
        assert_eq!(ui.shown(), ["  ●  Name  >ab"]);
    }

    fn run_select(question: &str, options: &[&str], ui: &mut Ui) -> Result<usize> {
        Select::new(question).options(options.iter().copied()).run(ui)
    }

    fn run_multiselect(question: &str, options: &[&str], ticked: &[usize], ui: &mut Ui) -> Result<Vec<usize>> {
        MultiSelect::new(question)
            .options(options.iter().copied())
            .default(ticked)
            .run(ui)
    }

    const LONG: [&str; 10] = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet",
    ];

    #[test]
    fn select_moves_and_wraps() {
        let pick = |keys: &[Key]| run_select("Pick", &["a", "b", "c"], &mut Ui::script(keys));
        assert_eq!(pick(&[Key::Down, Key::Down, Key::Enter]).unwrap(), 2);
        assert_eq!(pick(&[Key::Up, Key::Enter]).unwrap(), 2);
        assert_eq!(pick(&[Key::Char('j'), Key::Enter]).unwrap(), 1);
    }

    #[test]
    fn esc_clears_the_filter_before_cancelling() {
        let mut keys = typed("ho");
        // the cursor stays on what it was on
        keys.extend([Key::Esc, Key::Down, Key::Enter]);
        assert_eq!(run_select("Pick", &LONG, &mut Ui::script(&keys)).unwrap(), 5);
        let mut ui = Ui::script(&typed("ho"));
        let _ = run_select("Pick", &LONG, &mut ui);
        assert_eq!(ui.shown()[0], "  ?  Pick  ho▏  2 of 10");
        assert_eq!(ui.shown()[1], "    › echo");
        let mut keys = typed("ho");
        keys.extend([Key::Esc, Key::Esc]);
        assert!(matches!(
            run_select("Pick", &LONG, &mut Ui::script(&keys)),
            Err(Error::Cancelled)
        ));
        let mut keys = typed("zz");
        keys.extend([Key::Esc, Key::Space, Key::Enter]);
        assert_eq!(
            run_multiselect("Pick", &LONG, &[], &mut Ui::script(&keys)).unwrap(),
            [0]
        );
    }

    #[test]
    fn multiselect_toggles() {
        let keys = [Key::Space, Key::Down, Key::Space, Key::Down, Key::Space, Key::Enter];
        let mut ui = Ui::script(&keys);
        assert_eq!(
            run_multiselect("Pick", &["a", "b", "c"], &[1], &mut ui).unwrap(),
            [0, 2]
        );
        assert_eq!(ui.shown(), ["  ●  Pick  a, c"]);
        let keys = [Key::Char('a'), Key::Enter];
        assert_eq!(
            run_multiselect("Pick", &["a", "b"], &[], &mut Ui::script(&keys)).unwrap(),
            [0, 1]
        );
    }

    #[test]
    fn confirm_reads_piped_answers() {
        for (line, want) in [("y", true), ("YES", true), (" no ", false), ("N", false)] {
            assert_eq!(yes_or_no(line, false).unwrap(), want, "{line}");
        }
        // an empty line, or nothing at all, takes the default
        assert!(yes_or_no("", true).unwrap());
        assert!(!yes_or_no("", false).unwrap());
        assert!(matches!(yes_or_no("maybe", true), Err(Error::Invalid(_))));
    }

    #[test]
    fn going_back_is_opt_in() {
        let go = Confirm::new("Go?");
        assert!(!go.run(&mut Ui::script(&[Key::Left, Key::Enter])).unwrap());
        let mut ui = Ui::script(&[Key::Left]);
        assert!(matches!(go.back().run(&mut ui), Err(Error::Back)));
        // going back leaves nothing behind
        assert!(ui.shown().is_empty());

        let name = Input::new("Name");
        let mut keys = typed("ab");
        keys.extend([Key::Left, Key::Left, Key::Left, Key::Enter]);
        assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "ab");
        let name = name.back();
        assert!(matches!(name.run(&mut Ui::script(&keys)), Err(Error::Back)));
        // ← only goes back from the start of the line
        let mut keys = typed("ab");
        keys.extend([Key::Left, Key::Char('>'), Key::Enter]);
        assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "a>b");
    }

    fn services<'a>() -> Select<'a> {
        Select::new("Service")
            .heading("Frontend")
            .option("web")
            .disabled("docs", "no deploys on Fridays")
            .heading("Backend")
            .option("api")
            .disabled("cron", "retired")
            .option("worker")
    }

    #[test]
    fn headings_and_disabled_options_are_skipped() {
        let mut ui = Ui::script(&[Key::Down]);
        let _ = services().run(&mut ui);
        assert_eq!(
            ui.shown(),
            [
                "  ?  Service",
                "    Frontend",
                "      web",
                "      docs  (no deploys on Fridays)",
                "    Backend",
                "    › api",
                "      cron  (retired)",
                "      worker",
                "    ↑↓ move · enter choose",
            ]
        );
        let pick = |keys: &[Key]| services().run(&mut Ui::script(keys)).unwrap();
        assert_eq!(pick(&[Key::Down, Key::Down, Key::Enter]), 4);
        assert_eq!(pick(&[Key::Up, Key::Enter]), 4);
        assert_eq!(pick(&[Key::Down, Key::Down, Key::Down, Key::Enter]), 0);
        // a disabled default isn't where the cursor starts
        assert_eq!(services().default(1).run(&mut Ui::script(&[Key::Enter])).unwrap(), 0);
        assert_eq!(services().default(2).run(&mut Ui::script(&[Key::Enter])).unwrap(), 2);
    }

    #[test]
    fn multiselect_never_ticks_disabled_options() {
        let restart = || {
            MultiSelect::new("Restart")
                .option("api")
                .disabled("cron", "retired")
                .option("worker")
                .default(&[1])
        };
        let mut ui = Ui::script(&[Key::Down, Key::Space, Key::Enter]);
        assert_eq!(restart().run(&mut ui).unwrap(), [2]);
        assert_eq!(ui.shown(), ["  ●  Restart  worker"]);
        assert_eq!(
            restart().run(&mut Ui::script(&[Key::Char('a'), Key::Enter])).unwrap(),
            [0, 2]
        );
        let mut ui = Ui::script(&[Key::Down]);
        let _ = restart().run(&mut ui);
        assert_eq!(ui.shown()[2], "      – cron  (retired)");
    }

    #[test]
    fn validation_keeps_the_question_open() {
        let at_least_one = MultiSelect::new("Restart").options(["api", "worker"]).validate(|p| {
            if p.is_empty() {
                Err("pick at least one".into())
            } else {
                Ok(())
            }
        });
        let mut ui = Ui::script(&[Key::Enter]);
        assert!(at_least_one.run(&mut ui).is_err());
        assert_eq!(ui.shown()[1], "     ✕ pick at least one");
        let keys = [Key::Enter, Key::Space, Key::Enter];
        assert_eq!(at_least_one.run(&mut Ui::script(&keys)).unwrap(), [0]);

        let small = Select::new("Size")
            .options(["small", "large"])
            .validate(|i| if i == 1 { Err("too big".into()) } else { Ok(()) });
        let mut ui = Ui::script(&[Key::Down, Key::Enter]);
        assert!(small.run(&mut ui).is_err());
        assert_eq!(ui.shown()[1], "     ✕ too big");
        let keys = [Key::Down, Key::Enter, Key::Up, Key::Enter];
        assert_eq!(small.run(&mut Ui::script(&keys)).unwrap(), 0);
    }

    #[test]
    fn lists_go_back_and_show_how() {
        let mut ui = Ui::script(&[Key::Down]);
        let _ = services().back().run(&mut ui);
        assert_eq!(ui.shown().last().unwrap(), "    ↑↓ move · enter choose · ← back");
        assert!(matches!(
            services().back().run(&mut Ui::script(&[Key::Left])),
            Err(Error::Back)
        ));
        let restart = MultiSelect::new("Restart").options(["api"]).back();
        assert!(matches!(restart.run(&mut Ui::script(&[Key::Left])), Err(Error::Back)));
        let tag = Dropdown::new("Tag").options(["v1.0.0"]).back();
        let mut ui = Ui::script(&[Key::Left]);
        assert!(matches!(tag.run(&mut ui), Err(Error::Back)));
        assert!(ui.shown().is_empty());
        // without .back(), ← does nothing
        assert_eq!(services().run(&mut Ui::script(&[Key::Left, Key::Enter])).unwrap(), 0);
    }

    #[test]
    fn hints_sit_under_the_question() {
        let mut ui = Ui::script(&[]);
        let _ = Select::new("Region")
            .hint("where it runs")
            .options(["eu", "us"])
            .run(&mut ui);
        assert_eq!(ui.shown()[..3], ["  ?  Region", "    where it runs", "    › eu"]);
    }

    fn tags<'a>() -> Dropdown<'a> {
        let mut tags = Dropdown::new("Version")
            .option("v1.12.0")
            .note("today")
            .badge("latest")
            .option("v1.12.0-rc.1")
            .note("2 days ago")
            .dim();
        for minor in (0..12).rev() {
            tags = tags
                .option(format!("v1.{minor}.0"))
                .note(format!("{} weeks ago", 13 - minor));
        }
        tags.rows(5)
    }

    #[test]
    fn dropdowns_scroll_with_counts_above_and_below() {
        let shown = |keys: &[Key], default: Option<usize>| {
            let mut tags = tags();
            if let Some(d) = default {
                tags = tags.default(d);
            }
            let mut ui = Ui::script(keys);
            let _ = tags.run(&mut ui);
            ui.shown()
        };
        assert_eq!(
            shown(&[], Some(4)),
            [
                "  ?  Version",
                "      ↑ 2 more",
                "      v1.11.0            2 weeks ago",
                "      v1.10.0            3 weeks ago",
                "    › v1.9.0             4 weeks ago",
                "      v1.8.0             5 weeks ago",
                "      v1.7.0             6 weeks ago",
                "      ↓ 7 more",
                "    type to filter · ↑↓ move · enter choose",
            ]
        );
        assert_eq!(
            shown(&[], None)[1..5],
            [
                "",
                "    › v1.12.0 [latest]         today",
                "      v1.12.0-rc.1        2 days ago",
                "      v1.11.0            2 weeks ago",
            ]
        );
        assert_eq!(shown(&[Key::Up], Some(4))[4], "    › v1.10.0            3 weeks ago");
        assert_eq!(
            shown(&[Key::PageDown], Some(4))[4],
            "    › v1.4.0             9 weeks ago"
        );
        // ↑ from the top wraps to the bottom
        assert_eq!(
            shown(&[Key::Up], None)[1..8],
            [
                "      ↑ 9 more",
                "      v1.4.0             9 weeks ago",
                "      v1.3.0            10 weeks ago",
                "      v1.2.0            11 weeks ago",
                "      v1.1.0            12 weeks ago",
                "    › v1.0.0            13 weeks ago",
                "",
            ]
        );
    }

    #[test]
    fn dropdowns_filter_and_take_typed_answers() {
        let mut keys = typed("1.1");
        keys.push(Key::Enter);
        let mut ui = Ui::script(&keys);
        assert_eq!(tags().run(&mut ui).unwrap(), "v1.12.0");
        assert_eq!(ui.shown(), ["  ●  Version  v1.12.0"]);
        let mut ui = Ui::script(&typed("1.1"));
        let _ = tags().run(&mut ui);
        assert_eq!(ui.shown()[0], "  ?  Version  1.1▏  5 of 14");

        // a typed answer is only offered when asked for
        let mut keys = typed("v2.0.0");
        keys.push(Key::Enter);
        assert!(tags().run(&mut Ui::script(&keys)).is_err());
        let custom = tags().allow_custom().validate(|s| {
            if s.starts_with('v') {
                Ok(())
            } else {
                Err("tags start with v".into())
            }
        });
        let mut ui = Ui::script(&keys);
        assert_eq!(custom.run(&mut ui).unwrap(), "v2.0.0");
        assert_eq!(ui.shown(), ["  ●  Version  v2.0.0"]);
        let mut ui = Ui::script(&typed("v2.0.0"));
        let _ = custom.run(&mut ui);
        assert_eq!(ui.shown()[1], "    › use “v2.0.0”");
        let mut keys = typed("2.0.0");
        keys.push(Key::Enter);
        let mut ui = Ui::script(&keys);
        assert!(custom.run(&mut ui).is_err());
        assert_eq!(ui.shown()[1], "     ✕ tags start with v");
        // Esc clears what's typed, then cancels
        let mut keys = typed("zz");
        keys.extend([Key::Esc, Key::Enter]);
        assert_eq!(custom.run(&mut Ui::script(&keys)).unwrap(), "v1.12.0");
        let mut keys = typed("zz");
        keys.extend([Key::Esc, Key::Esc]);
        assert!(matches!(custom.run(&mut Ui::script(&keys)), Err(Error::Cancelled)));
    }

    #[test]
    fn without_a_terminal_lists_answer_with_their_default() {
        assert!(matches!(services().unasked(), Err(Error::NotATerminal)));
        assert_eq!(services().default(2).unasked().unwrap(), 2);
        // a default that can't be picked is no answer
        assert!(matches!(services().default(1).unasked(), Err(Error::NotATerminal)));
        let picky = services().default(2).validate(|_| Err("no".into()));
        assert!(matches!(picky.unasked(), Err(Error::Invalid(_))));

        let restart = || MultiSelect::new("Restart").option("api").disabled("cron", "retired");
        assert!(matches!(restart().unasked(), Err(Error::NotATerminal)));
        assert_eq!(restart().default(&[0, 1]).unasked().unwrap(), [0]);
        assert_eq!(restart().default(&[]).unasked().unwrap(), [] as [usize; 0]);
        let at_least_one = restart()
            .default(&[])
            .validate(|p| if p.is_empty() { Err("pick one".into()) } else { Ok(()) });
        assert!(matches!(at_least_one.unasked(), Err(Error::Invalid(_))));

        assert!(matches!(tags().unasked(), Err(Error::NotATerminal)));
        assert_eq!(tags().default(1).unasked().unwrap(), "v1.12.0-rc.1");
    }

    #[test]
    fn summaries_keep_whole_names() {
        assert_eq!(summary(&[]), "none");
        assert_eq!(summary(&["api", "web"]), "api, web");
        let many = ["api", "web", "worker", "scheduler", "gateway", "search", "billing"];
        assert_eq!(summary(&many), "api, web, worker, scheduler and 3 more");
        assert!(summary(&many).len() <= 40);
    }
}

//! Questions: yes/no, a line of text, a password, a number, a date, one of a list, several
//! of a list, or a longer answer in the user's editor.
//!
//! Drawn on stderr, so stdout stays clean for piping. Each question collapses to a single
//! `●  question  answer` line once answered.
//!
//! Every question returns a [`Result`], whose error says what happened in a way you can
//! match on: [`Error::Cancelled`] when someone presses Ctrl-C or Esc, [`Error::NotATerminal`]
//! when there's no one to ask (no terminal on stdin, or none on stderr to show the question
//! on). Without a terminal on stdin, `confirm` and the text questions (`input`, `number`,
//! `date`) read a line, so scripts can still drive them (`echo yes | my-cli`); an answer
//! read that way that doesn't make sense or fails validation is an [`Error::Invalid`].
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
//! [`Select`] and [`MultiSelect`] answer with the numbers of the options chosen, so the same
//! label can appear twice; [`Dropdown`] answers with the text, since an answer typed in
//! has no number. All three start on the options given to `selected`.
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
//! A [`Form`] asks several questions as one, going back and forth between them, and shows
//! all the answers at the end to change any of them.
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
use crate::style::{bold, italic, mix, paint, paint_on, underline};
use crate::term::{RawMode, read_byte};
use crate::text;

/// Why a question didn't get an answer.
#[derive(Debug)]
#[non_exhaustive]
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
    /// There's no terminal to ask on (stdin is a pipe or a file, or stderr, where questions
    /// are drawn, isn't a terminal), and this question has no sensible non-interactive
    /// answer.
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

mod date;
mod editor;
mod form;
mod input;
mod list;
mod script;
#[cfg(test)]
mod tests;
mod ui;

use self::ui::*;
pub use self::{date::*, editor::*, form::*, input::*, list::*, script::*};

//! Yes or no, and a line of text.

use super::*;

/// Yes or no. Enter takes the default. [`Confirm`] has more options.
///
/// Without a terminal it reads a line instead: `y`, `yes`, `n` or `no` in any case, and an
/// empty line (or the end of the input) takes the default. Anything else is an
/// [`Error::Invalid`], unless the question is [`lenient`](Confirm::lenient).
///
/// ```no_run
/// if tinsel::prompt::confirm("Deploy to production?", false)? {
///     tinsel::success("deploying");
/// }
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] when there's no terminal to ask
/// on and no line to read, [`Error::Invalid`] when a line read from a pipe isn't an answer it
/// takes, and [`Error::Io`] when the terminal can't be used.
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
#[derive(Clone, Debug)]
pub struct Confirm {
    question: String,
    default: bool,
    back: bool,
    at_end: Option<bool>,
    lenient: bool,
}

impl Confirm {
    /// A yes or no question.
    pub fn new(question: impl Into<String>) -> Self {
        Confirm {
            question: question.into(),
            default: false,
            back: false,
            at_end: None,
            lenient: false,
        }
    }

    /// The answer when Enter or → is pressed (or an empty line is piped in).
    #[must_use]
    pub fn default(mut self, yes: bool) -> Self {
        self.default = yes;
        self
    }

    /// Let ← go back to the question before, as [`Error::Back`].
    #[must_use]
    pub fn back(mut self) -> Self {
        self.back = true;
        self
    }

    /// The answer when there's no terminal and the piped input has already ended, so a
    /// script that runs out of answers can say no. The [`default`](Confirm::default) unless set.
    #[must_use]
    pub fn end_of_input(mut self, yes: bool) -> Self {
        self.at_end = Some(yes);
        self
    }

    /// Read a piped answer the way shell scripts often do: anything starting with `y` or `Y`
    /// is yes, anything else is no, and an empty line takes the [`default`](Confirm::default).
    /// Without it, only `y`, `yes`, `n` and `no` (in any case) are answers, and anything else
    /// is an [`Error::Invalid`]. Keys typed at a terminal work the same either way.
    ///
    /// ```no_run
    /// use tinsel::prompt::Confirm;
    ///
    /// // `echo yep | my-cli` says yes, `echo nope | my-cli` says no
    /// let go = Confirm::new("Go?").lenient().ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    #[must_use]
    pub fn lenient(mut self) -> Self {
        self.lenient = true;
        self
    }

    /// Ask the question. Without a terminal, reads the answer from stdin as [`confirm`] does.
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::Back`] on ← when asked with `.back()`,
    /// [`Error::NotATerminal`] when there's no terminal to ask on and no line to read,
    /// [`Error::Invalid`] when a line read from a pipe isn't an answer it takes, and [`Error::Io`] when
    /// the terminal can't be used.
    pub fn ask(self) -> Result<bool> {
        if !interactive() {
            return match read_piped()? {
                Some(line) => yes_or_no(&line, self.default, self.lenient),
                None => Ok(self.at_end.unwrap_or(self.default)),
            };
        }
        self.run(&mut Ui::open()?)
    }

    pub(super) fn run(&self, ui: &mut Ui) -> Result<bool> {
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

/// A yes or no answer read from a pipe. Leniently, anything but an empty line is an answer.
pub(super) fn yes_or_no(line: &str, default: bool, lenient: bool) -> Result<bool> {
    let answer = line.trim().to_lowercase();
    if lenient && !answer.is_empty() {
        return Ok(answer.starts_with('y'));
    }
    match answer.as_str() {
        "" => Ok(default),
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(Error::Invalid(format!("“{}” isn't yes or no", line.trim()))),
    }
}

// MARK: - Text

/// What a validation check returns: why the answer won't do, if it won't.
pub(super) type Checked = std::result::Result<(), String>;
pub(super) type ManyCheck<'a> = Box<dyn Fn(&[usize]) -> Checked + 'a>;
pub(super) type Validator<'a> = Box<dyn Fn(&str) -> std::result::Result<(), String> + 'a>;
pub(super) type Completer<'a> = Box<dyn Fn(&str) -> Vec<String> + 'a>;

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

impl std::fmt::Debug for Input<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("question", &self.question)
            .field("default", &self.default)
            .field("initial", &self.initial)
            .field("placeholder", &self.placeholder)
            .field("password", &self.secret)
            .field("back", &self.back)
            .finish_non_exhaustive()
    }
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
    ///     .suggestions(["eu-west-1", "eu-west-2", "us-east-1"])
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    #[must_use]
    pub fn suggestions<S: Into<String>>(self, options: impl IntoIterator<Item = S>) -> Self {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
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
    #[must_use]
    pub fn complete(mut self, complete: impl Fn(&str) -> Vec<String> + 'a) -> Self {
        self.complete = Some(Box::new(complete));
        self
    }

    /// Complete file and folder paths, like a shell: Tab fills in names from the folder
    /// typed so far (`~` is your home folder), and folders end in `/`.
    #[must_use]
    pub fn paths(self) -> Self {
        self.complete(complete_path)
    }

    /// The answer if Enter is pressed without typing anything, shown dim until you type.
    #[must_use]
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
    #[must_use]
    pub fn initial(mut self, text: impl Into<String>) -> Self {
        self.initial = Some(text.into());
        self
    }

    /// Dim example text shown until you type. Unlike a default, it's never the answer.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Show what's typed as dots.
    #[must_use]
    pub fn password(mut self) -> Self {
        self.secret = true;
        self
    }

    /// Check the answer when Enter is pressed. An `Err` is shown under the question, which
    /// is asked again. The default is checked too, if it's used.
    #[must_use]
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
    #[must_use]
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
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::Back`] on ← when asked with `.back()`,
    /// [`Error::NotATerminal`] when there's no terminal to ask on and no line to read,
    /// [`Error::Invalid`] when a line read from a pipe isn't an answer it takes, and [`Error::Io`] when
    /// the terminal can't be used.
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

    pub(super) fn run(&self, ui: &mut Ui) -> Result<String> {
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
pub(super) fn complete_path(typed: &str) -> Vec<String> {
    let home = std::env::var("HOME").or(std::env::var("USERPROFILE"));
    complete_path_in(typed, &home.unwrap_or_default())
}

/// [`complete_path`], with `~` standing for `home`.
pub(super) fn complete_path_in(typed: &str, home: &str) -> Vec<String> {
    let separators = ['/', std::path::MAIN_SEPARATOR];
    // only `~` on its own or before a separator is home; `~bob` is someone else's
    let home_rest = typed
        .strip_prefix('~')
        .filter(|rest| !home.is_empty() && (rest.is_empty() || rest.starts_with(separators)));
    if home_rest == Some("") {
        return vec![format!("~{}", std::path::MAIN_SEPARATOR)];
    }
    let expanded = match home_rest {
        Some(rest) => format!("{home}{rest}"),
        None => typed.to_string(),
    };
    // the folder and the name being typed, split in what was typed and in what it stands for
    let split = |s: &str| s.rfind(separators).map_or(0, |i| i + 1);
    let (dir, prefix) = expanded.split_at(split(&expanded));
    let typed_dir = &typed[..split(typed)];
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
pub(super) struct LineEdit {
    pub(super) text: String,
    /// The cursor, as a byte index on a char boundary.
    pub(super) at: usize,
}

impl LineEdit {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(super) fn split(&self) -> (&str, &str) {
        self.text.split_at(self.at)
    }

    /// Replace the text, with the cursor at the end.
    pub(super) fn set(&mut self, text: &str) {
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
    pub(super) fn key(&mut self, key: Key) -> bool {
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
            Key::WordLeft => {
                let before = self.text[..self.at].trim_end();
                self.at = before.rfind(' ').map_or(0, |i| i + 1);
                return false;
            }
            Key::WordRight => {
                let after = &self.text[self.at..];
                let skipped = after.len() - after.trim_start().len();
                let word = after[skipped..].find(' ').unwrap_or(after.len() - skipped);
                self.at += skipped + word;
                return false;
            }
            _ => return false,
        }
        true
    }
}

/// A line of text. An empty answer takes the default, shown dim until you type.
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] when there's no terminal to ask
/// on and no line to read, [`Error::Invalid`] when a line read from a pipe isn't an answer it
/// takes, and [`Error::Io`] when the terminal can't be used.
pub fn input(question: &str, default: Option<&str>) -> Result<String> {
    let mut q = Input::new(question);
    if let Some(d) = default {
        q = q.default(d);
    }
    q.ask()
}

/// A line of text shown as dots.
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] without a terminal, since a
/// password is never read from a pipe, and [`Error::Io`] when the terminal can't be used.
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
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] when there's no terminal to ask
/// on and no line to read, [`Error::Invalid`] when a line read from a pipe isn't an answer it
/// takes, and [`Error::Io`] when the terminal can't be used.
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
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] when there's no terminal to ask
/// on and no line to read, [`Error::Invalid`] when a line read from a pipe isn't an answer it
/// takes, and [`Error::Io`] when the terminal can't be used.
pub fn number<T>(question: &str, default: Option<T>) -> Result<T>
where
    T: FromStr + fmt::Display,
{
    let mut q = Input::new(question).validate(|s| {
        s.trim()
            .parse::<T>()
            .map(|_| ())
            .map_err(|_| format!("“{s}” isn't a {}", kind_of_number(std::any::type_name::<T>())))
    });
    if let Some(d) = default {
        q = q.default(d.to_string());
    }
    let answer = q.ask()?;
    // already checked, so it parses
    let parsed = answer.trim().parse().ok();
    parsed.ok_or(Error::Invalid(answer))
}

/// "whole number" or "number", from the type's name.
pub(super) fn kind_of_number(name: &str) -> &'static str {
    if name.starts_with('u') {
        "whole number of 0 or more"
    } else if name.starts_with('i') {
        "whole number"
    } else {
        "number"
    }
}

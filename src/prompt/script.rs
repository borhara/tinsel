//! Answering questions from a script of keys, for tests.

use super::*;

pub(super) struct Scripted {
    keys: std::collections::VecDeque<crate::app::Key>,
    pub(super) answered: Vec<String>,
}

thread_local! {
    pub(super) static SCRIPT: std::cell::RefCell<Option<Scripted>> = const { std::cell::RefCell::new(None) };
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
        use crate::app::{Key as K, NamedKey};
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
            // as a terminal sends them, which the questions read the same way
            K::Alt('b') | K::Mod(_, NamedKey::Left) => Key::WordLeft,
            K::Alt('f') | K::Mod(_, NamedKey::Right) => Key::WordRight,
            K::Mod(_, NamedKey::Backspace) => Key::Ctrl('w'),
            K::BackTab | K::F(_) | K::Alt(_) | K::Mod(..) => Key::Other,
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
#[derive(Debug)]
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

//! What every live display shares: how it decides to show itself, and its locks.

use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread;
use std::time::Duration;

use crate::output::Output;

/// A lock that a panic elsewhere doesn't spoil: a display carries on drawing what it has.
pub(crate) fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// [`lock`], giving up after about 100ms: for the panic hook, whose own thread might be the
/// one holding it.
pub(crate) fn lock_patiently<T: ?Sized>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    for _ in 0..50 {
        match m.try_lock() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(e)) => return Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(2)),
        }
    }
    None
}

/// How a display shows itself, decided once when it starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    /// Redrawn in place on a terminal.
    Draw,
    /// Plain lines, for logs and pipes.
    Plain,
    /// JSON events.
    Json,
    /// Only what needs attention.
    Quiet,
    /// Nothing drawn, on a clock or a snapshot that only moves when told: for tests.
    Headless,
}

impl Mode {
    /// How a display shows itself, given the output shape and whether it can animate.
    pub(crate) fn pick(out: Output, animated: bool) -> Mode {
        match out {
            Output::Json => Mode::Json,
            Output::Quiet => Mode::Quiet,
            Output::Pretty if animated => Mode::Draw,
            Output::Pretty => Mode::Plain,
        }
    }

    /// How a display starting now shows itself.
    pub(crate) fn now() -> Mode {
        Mode::pick(crate::output::output(), crate::output::is_terminal())
    }
}

//! What every live display shares: how it decides to show itself, its locks, and a list of
//! the ones drawing right now, so a question, a message or a panic can deal with them all.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak};
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
        Mode::pick(crate::output::output(), animated())
    }
}

/// A display drawing in place right now: a [`Live`](crate::Live) step list, a
/// [`Region`](crate::Region), or a spinner or [`Progress`](crate::Progress) line.
pub(crate) trait Shown: Send + Sync {
    /// Stop drawing, leaving the screen below the display free, and let go of stdin, until
    /// [`resume`](Shown::resume). Pauses nest: each one is undone by one resume.
    fn pause(&self);
    fn resume(&self);
    /// Print a line above the display, which carries on drawing below it.
    fn log(&self, line: &str);
    /// A panic is taking over the screen: leave it tidy, and never draw again.
    fn abandon(&self);
}

/// Every display drawing right now, oldest first.
static SHOWN: Mutex<Vec<Weak<dyn Shown>>> = Mutex::new(Vec::new());
/// How many there are, read without a lock on the way out of the program.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Note a display that has started drawing.
pub(crate) fn add(shown: Weak<dyn Shown>) {
    let mut all = lock(&SHOWN);
    all.retain(|w| w.strong_count() > 0);
    all.push(shown);
    ACTIVE.store(all.len(), Ordering::SeqCst);
}

/// Forget a display that has stopped drawing.
pub(crate) fn remove<T: ?Sized>(shown: *const T) {
    let mut all = lock(&SHOWN);
    all.retain(|w| w.strong_count() > 0 && !std::ptr::addr_eq(w.as_ptr(), shown));
    ACTIVE.store(all.len(), Ordering::SeqCst);
}

/// Whether anything is drawing in place, without taking a lock.
pub(crate) fn any_active() -> bool {
    ACTIVE.load(Ordering::SeqCst) > 0
}

fn shown() -> Vec<Arc<dyn Shown>> {
    lock(&SHOWN).iter().filter_map(Weak::upgrade).collect()
}

/// Every drawing display, held still while a question is asked or an editor is open over it.
///
/// A display and a question both want the terminal. The display redraws itself in place many
/// times a second, which paints over the question, and a step list's focus watch reads stdin,
/// which takes the keys meant for the question; with both going, a typed answer went nowhere
/// and the terminal was left in whichever mode the last one to let go had set. So each
/// display stops where it is (a step list leaves its last frame, a region or a spinner clears
/// itself away), its watch stops and hands the terminal back, and nothing is drawn until the
/// question is done. Then it draws on below the answer.
pub(crate) fn pause_all() -> Paused {
    let shown = shown();
    for d in &shown {
        d.pause();
    }
    Paused { shown }
}

/// Displays held still; they carry on when this is dropped.
pub(crate) struct Paused {
    shown: Vec<Arc<dyn Shown>>,
}

impl Drop for Paused {
    fn drop(&mut self) {
        for d in &self.shown {
            d.resume();
        }
    }
}

/// Print `lines` above the newest display drawing in place, if one is, so its next frame
/// doesn't draw over them; `stderr` says they're meant for stderr rather than the stream
/// tinsel draws on. Returns whether a display took them: if not, print them as usual.
///
/// Lines for stderr go above a display on stdout only when stderr is the same terminal;
/// when it's a file they go there as usual, and the display isn't disturbed.
pub(crate) fn print_above<S: AsRef<str>>(lines: &[S], stderr: bool) -> bool {
    if !any_active() {
        return false;
    }
    let same_screen = !stderr
        || crate::output::target() == crate::Target::Stderr
        || crate::output::pretended_terminal().unwrap_or_else(|| std::io::IsTerminal::is_terminal(&std::io::stderr()));
    if !same_screen {
        return false;
    }
    let Some(newest) = shown().pop() else {
        return false;
    };
    for line in lines {
        for part in line.as_ref().split('\n') {
            newest.log(part);
        }
    }
    true
}

/// Stop every display mid-draw, for a panic. Returns whether there were any.
pub(crate) fn abandon_all() -> bool {
    // the panicking thread might be holding the list, so never wait forever
    let Some(mut all) = lock_patiently(&SHOWN) else {
        return false;
    };
    let taken = std::mem::take(&mut *all);
    ACTIVE.store(0, Ordering::SeqCst);
    drop(all);
    let mut any = false;
    for d in taken.iter().filter_map(Weak::upgrade) {
        d.abandon();
        any = true;
    }
    any
}

/// Whether to redraw in place: on a terminal that can move its cursor. `TERM=dumb` (Emacs
/// shell buffers, some CI runners) is a terminal that can't, so it gets plain lines.
pub(crate) fn animated() -> bool {
    if let Some(terminal) = crate::output::pretended_terminal() {
        return terminal;
    }
    crate::output::is_terminal() && !dumb(std::env::var_os("TERM").as_deref())
}

fn dumb(term: Option<&std::ffi::OsStr>) -> bool {
    term.is_some_and(|t| t == "dumb")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dumb_terminal_gets_plain_lines() {
        assert!(dumb(Some("dumb".as_ref())));
        assert!(!dumb(Some("xterm-256color".as_ref())));
        assert!(!dumb(None));
        assert_eq!(Mode::pick(Output::Pretty, false), Mode::Plain);
        assert_eq!(Mode::pick(Output::Json, true), Mode::Json);
        assert!(crate::output::with_terminal(true, animated));
        assert!(!crate::output::with_terminal(false, animated));
    }

    #[test]
    fn a_question_holds_regions_and_spinners_still_too() {
        use crate::output::{capture, with_terminal, write};
        use std::time::Duration;
        crate::set_color_mode(crate::ColorMode::None);
        let out = capture(|| {
            with_terminal(true, || {
                let region = crate::Region::new(|| vec!["h-region".to_string()].into()).fps(240);
                let progress = crate::Progress::new("h-progress", 10).fps(240);
                thread::sleep(Duration::from_millis(30));
                let paused = pause_all();
                write("[held]");
                thread::sleep(Duration::from_millis(40));
                write("[free]");
                drop(paused);
                thread::sleep(Duration::from_millis(30));
                progress.finish();
                region.finish();
            })
        });
        let held = &out[out.find("[held]").unwrap()..out.find("[free]").unwrap()];
        assert!(!held.contains("h-region") && !held.contains("h-progress"), "{held:?}");
        let after = &out[out.find("[free]").unwrap()..];
        assert!(after.contains("h-region") && after.contains("h-progress"), "{after:?}");
        assert!(!any_active(), "both let go when they finished");
    }

    #[test]
    fn a_message_goes_above_a_region_rather_than_under_its_next_frame() {
        use crate::output::{capture, with_terminal};
        crate::set_color_mode(crate::ColorMode::None);
        let out = capture(|| {
            with_terminal(true, || {
                let region = crate::Region::new(|| vec!["m-region".to_string()].into()).fps(240);
                thread::sleep(Duration::from_millis(20));
                crate::success("m-message");
                crate::Block::from(vec!["m-block".to_string()]).print();
                thread::sleep(Duration::from_millis(20));
                region.finish();
            })
        });
        // printed by the region's frame, which clears the rest of each line it writes
        assert!(out.contains("m-message\x1b[K\n"), "{out:?}");
        assert!(out.contains("m-block\x1b[K\n"), "{out:?}");
    }
}

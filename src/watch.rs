//! Knowing whether anyone's looking. Terminals that support focus reporting (iTerm2, kitty,
//! WezTerm, Ghostty, Alacritty, foot, …) send `ESC [ I` when their window gains focus and
//! `ESC [ O` when it loses it. Reading those means reading stdin, so anything else typed
//! meanwhile is kept and handed back to the terminal afterwards.

use std::io::{self, IsTerminal, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{output, term};

/// How long someone has to be away before their return counts as coming back to look.
const AWAY_LONG_ENOUGH: Duration = Duration::from_secs(3);
/// How long the "here's what finished while you were away" twinkle lasts.
const WELCOME_BACK: Duration = Duration::from_millis(600);

#[derive(Default)]
struct Times {
    away_since: Option<Instant>,
    /// The last absence long enough to count: (left, came back).
    last_absence: Option<(Instant, Instant)>,
}

pub(crate) struct Focus {
    focused: AtomicBool,
    /// Whether the terminal has ever reported focus. If it hasn't, it may not support
    /// reporting at all, so we never assume someone's away.
    heard: AtomicBool,
    times: Mutex<Times>,
    typed: Mutex<Vec<u8>>,
}

impl Focus {
    /// Whether the terminal has said its window isn't focused.
    pub(crate) fn away(&self) -> bool {
        self.heard.load(Ordering::Relaxed) && !self.focused.load(Ordering::Relaxed)
    }

    /// If someone just came back after a while, when they left and when they returned.
    pub(crate) fn just_returned(&self, now: Instant) -> Option<(Instant, Instant)> {
        let times = self.times.lock().unwrap_or_else(|e| e.into_inner());
        times
            .last_absence
            .filter(|(_, back)| now.saturating_duration_since(*back) < WELCOME_BACK)
    }

    fn focus_changed(&self, focused: bool) {
        self.heard.store(true, Ordering::Relaxed);
        self.focused.store(focused, Ordering::Relaxed);
        let mut times = self.times.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if !focused {
            times.away_since.get_or_insert(now);
        } else if let Some(left) = times.away_since.take() {
            if now - left >= AWAY_LONG_ENOUGH {
                times.last_absence = Some((left, now));
            }
        }
    }
}

pub(crate) struct Watch {
    pub(crate) focus: Arc<Focus>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    saved: String,
}

impl Watch {
    /// Start listening for focus changes, if someone's typing at a terminal we're drawing on
    /// and the `TINSEL_NO_FOCUS` environment variable isn't set.
    pub(crate) fn start() -> Option<Watch> {
        if !io::stdin().is_terminal() || !output::is_terminal() || std::env::var_os("TINSEL_NO_FOCUS").is_some() {
            return None;
        }
        term::remember_terminal();
        let saved = term::enter_cbreak().ok()?;
        output::write("\x1b[?1004h");

        let focus = Arc::new(Focus {
            focused: AtomicBool::new(true),
            heard: AtomicBool::new(false),
            times: Mutex::new(Times::default()),
            typed: Mutex::new(Vec::new()),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (f, s) = (focus.clone(), stop.clone());
        let thread = thread::spawn(move || listen(&f, &s));
        Some(Watch {
            focus,
            stop,
            thread: Some(thread),
            saved,
        })
    }

    /// Stop listening, put the terminal back, and return anything typed meanwhile to it.
    pub(crate) fn stop(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        self.stop.store(true, Ordering::Relaxed);
        let _ = thread.join();
        output::write("\x1b[?1004l");
        let _ = term::stty(&[&self.saved]);
        term::forget_terminal();
        let typed = std::mem::take(&mut *self.focus.typed.lock().unwrap_or_else(|e| e.into_inner()));
        term::push_back_input(&typed);
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Read stdin until stopped, sorting focus reports from everything else.
fn listen(focus: &Focus, stop: &AtomicBool) {
    let mut stdin = io::stdin().lock();
    let mut pending: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let mut buf = [0u8; 64];
        // returns after at most 0.1s with nothing (stty time 1), so `stop` is checked often
        let n = stdin.read(&mut buf).unwrap_or(0);
        for &b in &buf[..n] {
            pending.push(b);
            match pending.as_slice() {
                b"\x1b[I" => {
                    focus.focus_changed(true);
                    pending.clear();
                }
                b"\x1b[O" => {
                    focus.focus_changed(false);
                    pending.clear();
                }
                // still possibly the start of a focus report: wait for more
                b"\x1b" | b"\x1b[" => {}
                other => {
                    focus
                        .typed
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .extend_from_slice(other);
                    pending.clear();
                }
            }
        }
    }
    if !pending.is_empty() {
        focus
            .typed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(&pending);
    }
}

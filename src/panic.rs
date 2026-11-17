//! Panics drawn like every other error, without leaving the terminal in a mess.

use std::io::Write;
use std::panic::PanicHookInfo;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::message::Report;
use crate::style::paint;
use crate::style::palette::MUTED;

/// Show panics as a tinsel [`Report`] instead of Rust's default message, and put the terminal
/// back first if a [`Live`](crate::Live) display or a [`Region`](crate::Region) was mid-draw: running steps are marked as
/// interrupted, the last frame is left on screen, and the cursor comes back.
///
/// Call it once, early in `main`; calling it again does nothing. A hook set before it still
/// runs, after the report. With `RUST_BACKTRACE` set the backtrace is printed under the
/// report, dimmed.
///
/// ```
/// tinsel::install_panic_hook();
/// ```
///
/// ```text
///   ✕  my-app panicked
///      ╰─ index out of bounds: the len is 3 but the index is 7
///         ╰─ at src/main.rs:42:13
///   →  run with RUST_BACKTRACE=1 to see where it came from
/// ```
pub fn install_panic_hook() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    // taking the hook puts Rust's own back, so taking it again shows what that looks like
    let default = std::panic::take_hook();
    let previous = (!std::ptr::eq(&*previous, &*default)).then_some(previous);
    std::panic::set_hook(Box::new(move |info| {
        hook(info);
        // a hook the program set before (crash reporting, say) still hears about it; Rust's
        // own would only say it all again
        if let Some(previous) = &previous {
            previous(info);
        }
    }));
}

fn hook(info: &PanicHookInfo<'_>) {
    let interrupted = crate::live::abandon_all() | crate::region::abandon_all();
    // with panic = "abort", no destructors run, so nothing else will put the terminal back
    cfg!(panic = "abort").then(crate::term::restore_saved_terminal);

    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panicked with a value that isn't text".into());
    let program = crate::history::History::default_app();
    let thread = std::thread::current();
    let title = match thread.name() {
        Some("main") | None => format!("{program} panicked"),
        Some(name) => format!("{program} panicked in thread '{name}'"),
    };
    let at = info
        .location()
        .map(|at| format!("at {}:{}:{}", at.file(), at.line(), at.column()));
    let mut report = at.into_iter().fold(Report::new(title).cause(message), Report::cause);

    let backtrace = std::backtrace::Backtrace::capture();
    let captured = backtrace.status() == std::backtrace::BacktraceStatus::Captured;
    if !captured {
        report = report.hint("run with RUST_BACKTRACE=1 to see where it came from");
    }
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if interrupted {
        eprintln!();
    }
    report.eprint();
    if captured && !crate::output::json() {
        for line in backtrace.to_string().lines() {
            eprintln!("     {}", paint(MUTED, line));
        }
    }
}

//! Panics drawn like every other error, without leaving the terminal in a mess.

use std::io::Write;
use std::panic::PanicHookInfo;

use crate::message::Report;
use crate::style::paint;
use crate::style::palette::MUTED;

/// Show panics as a tinsel [`Report`] instead of Rust's default message, and put the terminal
/// back first if a [`Live`](crate::Live) display was mid-draw: running steps are marked as
/// interrupted, the last frame is left on screen, and the cursor comes back.
///
/// Call it once, early in `main`. With `RUST_BACKTRACE` set the backtrace is printed under
/// the report, dimmed.
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
    std::panic::set_hook(Box::new(hook));
}

fn hook(info: &PanicHookInfo<'_>) {
    let interrupted = crate::live::abandon_all();
    // with panic = "abort", no destructors run, so nothing else will put the terminal back
    if cfg!(panic = "abort") {
        crate::term::restore_saved_terminal();
    }

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
    let mut report = Report::new(title).cause(message);
    if let Some(at) = info.location() {
        report = report.cause(format!("at {}:{}:{}", at.file(), at.line(), at.column()));
    }

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

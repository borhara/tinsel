//! Panics drawn like every other error, without leaving the terminal in a mess.

use std::io::Write;
use std::panic::PanicHookInfo;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::message::Report;
use crate::style::paint;
use crate::style::palette::MUTED;

/// Show panics as a tinsel [`Report`] instead of Rust's default message, and put the terminal
/// back first if a [`Live`](crate::Live) display or a [`Region`](crate::Region) was mid-draw: running steps are marked as
/// interrupted, the last frame is left on screen, and the cursor comes back. That's for a
/// panic that ends the program (one on the main thread, or any with `panic = "abort"`); one
/// on another thread, which the program may well survive, is reported above the displays and
/// they carry on.
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

/// Whether a panic here ends the program: one on the main thread does, unless it's caught,
/// which can't be told from here, and with panic = "abort" every one does. One on another
/// thread can be caught or joined (a pool's task, a worker), and the program carries on.
fn ends_the_program(thread: Option<&str>) -> bool {
    cfg!(panic = "abort") || thread == Some("main")
}

fn hook(info: &PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let fatal = ends_the_program(thread.name());
    // displays are only stopped for a panic that ends the program; any other one is reported
    // above them, and they carry on
    let interrupted = fatal && (crate::live::abandon_all() | crate::region::abandon_all());
    // with panic = "abort", no destructors run, so nothing else will put the terminal back
    cfg!(panic = "abort").then(crate::term::restore_saved_terminal);

    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panicked with a value that isn't text".into());
    let program = crate::history::History::default_app();
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
    let backtrace: Vec<String> = if captured {
        backtrace
            .to_string()
            .lines()
            .map(|line| format!("     {}", paint(MUTED, line)))
            .collect()
    } else {
        Vec::new()
    };
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if crate::output::json() {
        return report.eprint();
    }
    let mut lines = report.lines().into_lines();
    lines.extend(backtrace);
    // a display still drawing (this panic didn't stop it) takes the report above it; one that
    // can't be had in a moment is left alone, since this thread might be the one holding it
    if !fatal && crate::display::print_above_patiently(&lines, true) {
        return;
    }
    if interrupted {
        eprintln!();
    }
    for line in &lines {
        crate::output::err_line(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_panic_that_ends_the_program_stops_the_displays() {
        assert!(ends_the_program(Some("main")));
        assert_eq!(ends_the_program(Some("worker")), cfg!(panic = "abort"));
        assert_eq!(ends_the_program(None), cfg!(panic = "abort"));
    }
}

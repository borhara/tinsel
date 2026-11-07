//! What shape output takes (drawn for people, JSON lines for programs, or only what matters),
//! and where it goes.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicU8, Ordering};

/// What shape tinsel's output takes. Set it from a `--json` or `--quiet` flag with
/// [`set_output`], or with `TINSEL_OUTPUT=json|quiet`.
///
/// ```
/// tinsel::set_output(tinsel::Output::Json);
/// tinsel::success("deployed");    // {"event":"message","level":"success","text":"deployed"}
/// # tinsel::set_output(tinsel::Output::Pretty);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Output {
    /// For people: colour and animation on a terminal, plain `[ok] Build (1.2s)` lines elsewhere.
    #[default]
    Pretty,
    /// For programs: one JSON object per line on stdout (or the [`Target`]), for every message, step, report and
    /// block. Each has an `"event"` field saying what it is. Prompts still ask on the terminal.
    Json,
    /// Only what needs attention: warnings, errors, failed steps and anything printed
    /// explicitly (tables, trees and other blocks). Progress, spinners and success messages
    /// stay silent.
    Quiet,
}

// 0 = not decided yet; otherwise 1 + the Output's position
static OUTPUT: AtomicU8 = AtomicU8::new(0);

/// The output shape in use. Decided once from `TINSEL_OUTPUT`, unless [`set_output`] has chosen one.
pub fn output() -> Output {
    match OUTPUT.load(Ordering::Relaxed) {
        1 => Output::Pretty,
        2 => Output::Json,
        3 => Output::Quiet,
        _ => {
            let out = match std::env::var("TINSEL_OUTPUT").as_deref() {
                Ok("json") => Output::Json,
                Ok("quiet") => Output::Quiet,
                _ => Output::Pretty,
            };
            set_output(out);
            out
        }
    }
}

/// Choose the output shape, for a `--json` or `--quiet` flag.
pub fn set_output(out: Output) {
    let code = match out {
        Output::Pretty => 1,
        Output::Json => 2,
        Output::Quiet => 3,
    };
    OUTPUT.store(code, Ordering::Relaxed);
}

/// Where tinsel draws: stdout (the default) or stderr. Choose stderr when stdout belongs to
/// something else, like the output of a command your tool runs or data piped to another
/// program. Set it with [`set_target`] or `TINSEL_TARGET=stderr`.
///
/// ```
/// tinsel::set_target(tinsel::Target::Stderr);
/// tinsel::success("checked");    // on stderr, coloured if stderr is a terminal
/// # tinsel::set_target(tinsel::Target::Stdout);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Standard output.
    #[default]
    Stdout,
    /// Standard error.
    Stderr,
}

// 0 = not decided yet, 1 = stdout, 2 = stderr
static TARGET: AtomicU8 = AtomicU8::new(0);

/// Where tinsel draws. Decided once from `TINSEL_TARGET` (`stderr` or `stdout`), unless
/// [`set_target`] has chosen.
pub fn target() -> Target {
    match TARGET.load(Ordering::Relaxed) {
        1 => Target::Stdout,
        2 => Target::Stderr,
        _ => {
            let target = parse_target(std::env::var("TINSEL_TARGET").ok().as_deref());
            TARGET.store(target_code(target), Ordering::Relaxed);
            target
        }
    }
}

/// Send everything tinsel draws to stdout or stderr: [`Live`](crate::Live) displays,
/// [`Region`](crate::Region)s, spinners and [`Progress`](crate::Progress) bars, messages,
/// printed [`Block`](crate::Block)s and JSON events. Whether to draw in colour and animate
/// is then decided by whether that stream is a terminal. Errors and
/// [`Report`](crate::Report)s go to stderr either way, and prompts still ask on the terminal.
///
/// Call it early, before anything is drawn.
///
/// ```
/// tinsel::set_target(tinsel::Target::Stderr);
/// tinsel::spin("Checking", || std::thread::sleep(std::time::Duration::from_millis(10)));
/// # tinsel::set_target(tinsel::Target::Stdout);
/// ```
pub fn set_target(target: Target) {
    let before = TARGET.swap(target_code(target), Ordering::Relaxed);
    if before != target_code(target) {
        crate::style::forget_detected();
    }
}

fn target_code(target: Target) -> u8 {
    match target {
        Target::Stdout => 1,
        Target::Stderr => 2,
    }
}

fn parse_target(env: Option<&str>) -> Target {
    match env.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("stderr") => Target::Stderr,
        _ => Target::Stdout,
    }
}

/// Whether the stream `target` names is a terminal, asking only about that one.
fn on_terminal(target: Target, stdout: &dyn Fn() -> bool, stderr: &dyn Fn() -> bool) -> bool {
    match target {
        Target::Stdout => stdout(),
        Target::Stderr => stderr(),
    }
}

/// Whether the stream tinsel draws on is a terminal.
pub(crate) fn is_terminal() -> bool {
    on_terminal(target(), &|| std::io::stdout().is_terminal(), &|| {
        std::io::stderr().is_terminal()
    })
}

/// The file descriptor tinsel draws on.
pub(crate) fn fd() -> i32 {
    match target() {
        Target::Stdout => 1,
        Target::Stderr => 2,
    }
}

/// [`fd`] as already decided, deciding nothing: safe in a signal handler.
#[cfg(unix)]
pub(crate) fn fd_now() -> i32 {
    if TARGET.load(Ordering::Relaxed) == 2 { 2 } else { 1 }
}

/// The stream tinsel draws on.
pub(crate) fn writer() -> Box<dyn Write> {
    match target() {
        Target::Stdout => Box::new(std::io::stdout().lock()),
        Target::Stderr => Box::new(std::io::stderr().lock()),
    }
}

/// Write to the stream tinsel draws on, and flush.
pub(crate) fn write(s: &str) {
    let mut out = writer();
    let _ = out.write_all(s.as_bytes());
    let _ = out.flush();
}

/// Write a line to the stream tinsel draws on.
pub(crate) fn line(s: &str) {
    write(&format!("{s}\n"));
}

pub(crate) fn json() -> bool {
    output() == Output::Json
}

pub(crate) fn quiet() -> bool {
    output() == Output::Quiet
}

/// A value in a JSON event.
pub(crate) enum Val<'a> {
    Str(&'a str),
    Num(f64),
    List(&'a [String]),
}

/// Write one JSON event line to the stream tinsel draws on: `{"event":"…", …fields}`. Text is stripped of styling.
pub(crate) fn emit(event: &str, fields: &[(&str, Val)]) {
    let mut line = format!("{{\"event\":{}", quote(event));
    for (key, val) in fields {
        let _ = write!(line, ",{}:", quote(key));
        match val {
            Val::Str(s) => line += &quote(s),
            Val::Num(n) if n.is_finite() => line += &format!("{}", (n * 1000.0).round() / 1000.0),
            Val::Num(_) => line += "null",
            Val::List(items) => {
                let items: Vec<String> = items.iter().map(|s| quote(s)).collect();
                let _ = write!(line, "[{}]", items.join(","));
            }
        }
    }
    line += "}\n";
    write(&line);
}

/// A JSON string literal, styling removed.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in crate::text::strip_ansi(s).chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            '\t' => out += "\\t",
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::{Target, on_terminal, parse_target, quote};

    #[test]
    fn target_comes_from_the_environment() {
        assert_eq!(parse_target(None), Target::Stdout);
        assert_eq!(parse_target(Some("stderr")), Target::Stderr);
        assert_eq!(parse_target(Some(" STDERR ")), Target::Stderr);
        assert_eq!(parse_target(Some("stdout")), Target::Stdout);
        assert_eq!(parse_target(Some("nonsense")), Target::Stdout);
    }

    #[test]
    fn the_chosen_stream_decides_whether_to_draw() {
        let (yes, no) = (&|| true, &|| false);
        // stdout piped somewhere, stderr on the terminal
        assert!(!on_terminal(Target::Stdout, no, yes));
        assert!(on_terminal(Target::Stderr, no, yes));
        // and the other way round
        assert!(on_terminal(Target::Stdout, yes, no));
        assert!(!on_terminal(Target::Stderr, yes, no));
        // the other stream is never asked about
        assert!(on_terminal(Target::Stderr, &|| panic!("asked about stdout"), yes));
    }

    #[test]
    fn quotes_strings_safely() {
        assert_eq!(quote("a \"b\"\n\\"), r#""a \"b\"\n\\""#);
        assert_eq!(quote("\x1b[31mred\x1b[39m"), "\"red\"");
        assert_eq!(quote("\x01"), "\"\\u0001\"");
    }
}

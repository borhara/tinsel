//! What shape output takes: drawn for people, JSON lines for programs, or only what matters.

use std::fmt::Write as _;
use std::io::Write;
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
    /// For programs: one JSON object per line on stdout, for every message, step, report and
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

/// Write one JSON event line to stdout: `{"event":"…", …fields}`. Text is stripped of styling.
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
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
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
    use super::quote;

    #[test]
    fn quotes_strings_safely() {
        assert_eq!(quote("a \"b\"\n\\"), r#""a \"b\"\n\\""#);
        assert_eq!(quote("\x1b[31mred\x1b[39m"), "\"red\"");
        assert_eq!(quote("\x01"), "\"\\u0001\"");
    }
}

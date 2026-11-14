//! One-line messages, headings, callouts and error reports.

use std::fmt;

use crate::block::Block;
use crate::output::{self, Val};
use crate::style::palette::{AMBER, BLUSH, LINE, MUTED, RED, ROSE, SAGE, SOFT, TEXT};
use crate::style::{Rgb, bold, paint};
use crate::term::content_width;
use crate::text;

/// The kind of a one-line message. The free functions ([`success`], [`info`], …) print
/// straight away; use a `Tone` when you want the line as a string instead.
///
/// ```
/// use tinsel::Tone;
///
/// let line = Tone::Success.line("Deployed my-app");
/// assert!(tinsel::text::strip_ansi(&line).contains("Deployed my-app"));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// `●` in rose: something finished.
    Success,
    /// `›` in blush: something worth knowing.
    Info,
    /// `▲` in amber: something to look at.
    Warn,
    /// `✕` in red: something went wrong.
    Error,
    /// `→` in muted grey: what to do next.
    Hint,
}

impl Tone {
    /// The message as a styled line, without printing it.
    pub fn line(self, msg: &str) -> String {
        let (glyph, color, text_color): (&str, Rgb, Rgb) = match self {
            Tone::Success => ("●", ROSE, TEXT),
            Tone::Info => ("›", BLUSH, TEXT),
            Tone::Warn => ("▲", AMBER, TEXT),
            Tone::Error => ("✕", RED, TEXT),
            Tone::Hint => ("→", MUTED, SOFT),
        };
        format!("  {}  {}", paint(color, glyph), paint(text_color, msg))
    }

    /// Print the message: errors to stderr, everything else to stdout (or the
    /// [`Target`](crate::Target)). In
    /// [`Output::Quiet`](crate::Output::Quiet) only warnings and errors print.
    pub fn print(self, msg: &str) {
        if output::json() {
            return output::emit("message", &[("level", Val::Str(self.name())), ("text", Val::Str(msg))]);
        }
        if output::quiet() && !matches!(self, Tone::Warn | Tone::Error) {
            return;
        }
        match self {
            Tone::Error => output::err_line(&self.line(msg)),
            _ => output::line(&self.line(msg)),
        }
    }
}

impl Tone {
    fn name(self) -> &'static str {
        match self {
            Tone::Success => "success",
            Tone::Info => "info",
            Tone::Warn => "warn",
            Tone::Error => "error",
            Tone::Hint => "hint",
        }
    }
}

/// Prints `●  msg`: something finished.
pub fn success(msg: &str) {
    Tone::Success.print(msg);
}

/// Prints `›  msg`: something worth knowing.
pub fn info(msg: &str) {
    Tone::Info.print(msg);
}

/// Prints `▲  msg`: something to look at.
pub fn warn(msg: &str) {
    Tone::Warn.print(msg);
}

/// Prints `✕  msg` to stderr: something went wrong.
pub fn error(msg: &str) {
    Tone::Error.print(msg);
}

/// Prints `→  msg`, quieter than the rest: what to do next.
pub fn hint(msg: &str) {
    Tone::Hint.print(msg);
}

/// Prints `2/5  Building`, for scripts that announce steps as they go rather than drawing a
/// live list.
pub fn counter(n: usize, total: usize, msg: &str) {
    if output::json() {
        let fields = [
            ("n", Val::Num(n as f64)),
            ("total", Val::Num(total as f64)),
            ("text", Val::Str(msg)),
        ];
        return output::emit("counter", &fields);
    }
    if output::quiet() {
        return;
    }
    let w = total.to_string().len();
    output::line(&format!(
        "  {}  {}",
        paint(MUTED, &format!("{n:>w$}/{total}")),
        paint(TEXT, msg)
    ));
}

/// Prints a heading with an optional quieter subtitle beside it, with a blank line either side.
pub fn title(text: &str, subtitle: Option<&str>) {
    if output::json() {
        let sub = subtitle.unwrap_or_default();
        return output::emit("title", &[("text", Val::Str(text)), ("subtitle", Val::Str(sub))]);
    }
    if output::quiet() {
        return;
    }
    let sub = subtitle.map(|s| format!("  {}", paint(SOFT, s))).unwrap_or_default();
    output::line(&format!("\n  {}{sub}\n", bold(&paint(ROSE, text))));
}

/// A thin divider across the content width, with an optional label in it.
///
/// ```
/// let line = tinsel::rule("build");
/// assert!(tinsel::text::strip_ansi(&line).starts_with("  ── build ──"));
/// ```
pub fn rule(label: &str) -> String {
    let w = content_width().saturating_sub(2);
    if label.is_empty() {
        return format!("  {}", paint(LINE, &"─".repeat(w)));
    }
    let rest = w.saturating_sub(text::width(label) + 4);
    format!(
        "  {} {} {}",
        paint(LINE, "──"),
        paint(SOFT, label),
        paint(LINE, &"─".repeat(rest))
    )
}

/// The tone of a [`callout`] block, which sets its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    /// Rose: worth knowing.
    Note,
    /// Sage: a helpful suggestion.
    Tip,
    /// Amber: be careful.
    Warning,
    /// Red: something destructive or broken.
    Danger,
}

impl Callout {
    fn color(self) -> Rgb {
        match self {
            Callout::Note => ROSE,
            Callout::Tip => SAGE,
            Callout::Warning => AMBER,
            Callout::Danger => RED,
        }
    }
}

/// A block set off by a coloured bar on the left, with a title and a body wrapped to fit.
///
/// ```
/// use tinsel::{Callout, callout};
///
/// callout(Callout::Tip, "Faster rebuilds", "Pass --incremental to reuse the last build.").print();
/// ```
pub fn callout(kind: Callout, title: &str, body: &str) -> Block {
    let bar = paint(kind.color(), "▎");
    let mut out = Block::new();
    out.push(format!("  {bar} {}", bold(&paint(kind.color(), title))));
    for line in text::wrap(body, content_width().saturating_sub(6)) {
        out.push(format!("  {bar} {}", paint(SOFT, &line)));
    }
    out
}

/// An error, what caused it (outermost first), and a hint at the fix.
///
/// ```
/// use tinsel::Report;
///
/// let report = Report::new("couldn't deploy my-app")
///     .cause("AccessDenied: iam:PassRole")
///     .cause("the role has no trust policy for Lambda")
///     .hint("attach one with `my-app role fix`");
/// report.eprint();
/// assert!(report.lines().plain().contains("╰─ AccessDenied"));
/// ```
///
/// ```text
///   ✕  couldn't deploy my-app
///      ╰─ AccessDenied: iam:PassRole
///         ╰─ the role has no trust policy for Lambda
///   →  attach one with `my-app role fix`
/// ```
#[derive(Clone, Debug, Default)]
pub struct Report {
    title: String,
    causes: Vec<String>,
    code: Option<crate::layout::Code>,
    hint: Option<String>,
}

impl Report {
    /// A report headed by what went wrong.
    pub fn new(title: impl Into<String>) -> Self {
        Report {
            title: title.into(),
            ..Self::default()
        }
    }

    /// A report for any error, with its whole `source()` chain as causes.
    ///
    /// ```
    /// let err = std::io::Error::other("disk full");
    /// let report = tinsel::Report::from_error("couldn't save", &err);
    /// assert!(report.lines().plain().contains("disk full"));
    /// ```
    pub fn from_error(title: impl Into<String>, err: &dyn std::error::Error) -> Self {
        let mut report = Report::new(title).cause(err.to_string());
        let mut source = err.source();
        while let Some(e) = source {
            report = report.cause(e.to_string());
            source = e.source();
        }
        report
    }

    /// Add the next cause, one level deeper than the last.
    pub fn cause(mut self, cause: impl Into<String>) -> Self {
        self.causes.push(cause.into());
        self
    }

    /// Show where the problem is, as a [`Code`](crate::Code) excerpt under the causes.
    ///
    /// ```
    /// use tinsel::{Code, Report};
    ///
    /// let config = "[server]\nport = \"eighty\"\n";
    /// Report::new("couldn't read app.toml")
    ///     .cause("port must be a number")
    ///     .code(Code::around(config, 2, 1).path("app.toml").caret(8, 8, "not a number"))
    ///     .hint("use a port like 8080")
    ///     .eprint();
    /// ```
    pub fn code(mut self, code: crate::layout::Code) -> Self {
        self.code = Some(code);
        self
    }

    /// What to do about it.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The report as lines.
    pub fn lines(&self) -> Block {
        let mut out = Block::new();
        out.push(format!("  {}  {}", paint(RED, "✕"), bold(&paint(TEXT, &self.title))));
        for (depth, cause) in self.causes.iter().enumerate() {
            let indent = " ".repeat(5 + depth * 3);
            out.push(format!("{indent}{} {}", paint(LINE, "╰─"), paint(SOFT, cause)));
        }
        if let Some(code) = &self.code {
            for line in code.lines() {
                out.push(format!("   {line}"));
            }
        }
        if let Some(h) = &self.hint {
            out.push(Tone::Hint.line(h));
        }
        out
    }

    /// Print the report to stderr, where errors belong. As JSON it's an `"error"` event
    /// with `title`, `causes` and `hint`.
    pub fn eprint(&self) {
        if output::json() {
            let hint = self.hint.as_deref().unwrap_or_default();
            let code: Vec<String> = self
                .code
                .iter()
                .flat_map(|c| c.lines().plain().lines().map(String::from).collect::<Vec<_>>())
                .collect();
            let fields = [
                ("title", Val::Str(&self.title)),
                ("causes", Val::List(&self.causes)),
                ("code", Val::List(&code)),
                ("hint", Val::Str(hint)),
            ];
            return output::emit("error", &fields);
        }
        self.lines().eprint();
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lines().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::{Output, capture, with_output};

    #[test]
    fn messages_print_as_lines_events_or_not_at_all() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = capture(|| {
            success("built");
            info("cached");
            warn("slow");
            hint("try again");
            error("broke");
        });
        for want in ["  ●  built\n", "  ›  cached\n", "  ▲  slow\n", "  →  try again\n"] {
            assert!(out.contains(want), "{want:?} in {out:?}");
        }
        assert!(out.contains("broke\n"), "errors go to stderr, and are caught too");

        let quiet = capture(|| {
            with_output(Output::Quiet, || {
                Tone::Success.print("q-msg");
                Tone::Warn.print("q-msg");
            })
        });
        assert!(quiet.contains("▲  q-msg") && !quiet.contains("●  q-msg"), "{quiet:?}");

        let json = capture(|| with_output(Output::Json, || error("j-msg")));
        assert!(
            json.contains(r#"{"event":"message","level":"error","text":"j-msg"}"#),
            "{json}"
        );
    }

    #[test]
    fn counters_and_titles() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = capture(|| {
            counter(2, 10, "c-pretty");
            title("t-pretty", Some("sub"));
            title("t-bare", None);
        });
        assert!(out.contains("   2/10  c-pretty\n"), "{out:?}");
        assert!(
            out.contains("\n  t-pretty  sub\n\n") && out.contains("\n  t-bare\n\n"),
            "{out:?}"
        );

        let quiet = capture(|| {
            with_output(Output::Quiet, || {
                counter(1, 2, "c-quiet");
                title("t-quiet", None);
            })
        });
        assert!(!quiet.contains("c-quiet") && !quiet.contains("t-quiet"));

        let json = capture(|| {
            with_output(Output::Json, || {
                counter(1, 2, "c-json");
                title("t-json", None);
            })
        });
        assert!(
            json.contains(r#"{"event":"counter","n":1,"total":2,"text":"c-json"}"#),
            "{json}"
        );
        assert!(
            json.contains(r#"{"event":"title","text":"t-json","subtitle":""}"#),
            "{json}"
        );
    }

    #[test]
    fn a_rule_without_a_label_is_just_a_line() {
        crate::set_color_mode(crate::ColorMode::None);
        let line = rule("");
        assert!(line.trim().chars().all(|c| c == '─') && line.len() > 10);
    }

    #[test]
    fn every_callout_has_its_colour() {
        let colours: Vec<Rgb> = [Callout::Note, Callout::Tip, Callout::Warning, Callout::Danger]
            .iter()
            .map(|c| c.color())
            .collect();
        assert_eq!(colours, [ROSE, SAGE, AMBER, RED]);
    }

    #[derive(Debug)]
    struct Wrapped(std::io::Error);

    impl fmt::Display for Wrapped {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("couldn't read the config")
        }
    }

    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn reports_follow_the_whole_chain_and_show_code() {
        crate::set_color_mode(crate::ColorMode::None);
        let report = Report::from_error("couldn't start", &Wrapped(std::io::Error::other("disk full")))
            .code(crate::layout::Code::around("port = x\n", 1, 0).caret(8, 1, "here"));
        let text = report.to_string();
        assert!(
            text.contains("╰─ couldn't read the config") && text.contains("╰─ disk full"),
            "{text}"
        );
        assert!(text.contains("port = x") && text.contains("here"), "{text}");
        report.eprint();

        let json = capture(|| with_output(Output::Json, || report.clone().hint("fix it").eprint()));
        assert!(json.contains(r#""event":"error","title":"couldn't start""#), "{json}");
        assert!(
            json.contains(r#""hint":"fix it""#) && json.contains("port = x"),
            "{json}"
        );
    }

    #[test]
    fn a_rule_carries_its_label() {
        crate::set_color_mode(crate::ColorMode::None);
        assert!(rule("build").starts_with("  ── build ──"));
    }

    #[test]
    fn every_level_is_named_in_events() {
        let json = capture(|| {
            with_output(Output::Json, || {
                for print in [success, info, warn, hint] {
                    print("x");
                }
            })
        });
        for level in ["success", "info", "warn", "hint"] {
            assert!(json.contains(&format!(r#""level":"{level}""#)), "{level} in {json}");
        }
    }
}

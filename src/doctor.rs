//! What tinsel decided about the terminal, and why: for when it decides wrong.

use crate::block::Block;
use crate::message::Tone;
use crate::style::palette::{MUTED, SOFT, TEXT};
use crate::style::{Accent, ColorMode, Theme, bold, paint};
use crate::text;

/// Every setting tinsel reads from the environment.
const SETTINGS: [&str; 7] = [
    "TINSEL_THEME",
    "TINSEL_ACCENT",
    "TINSEL_OUTPUT",
    "TINSEL_TARGET",
    "TINSEL_FPS",
    "TINSEL_NO_FOCUS",
    "TINSEL_NO_HISTORY",
];

/// What tinsel decided about the terminal it's drawing on, and why: the colour mode, theme,
/// size, output shape, target, accent, frame rate, focus, history and notifications, each
/// with what decided it (`stdout isn't a terminal`, `COLORFGBG=15;0`, `set in code`). Then
/// a warning for each setting in the environment it can't use: a misspelt `TINSEL_` name,
/// or a value it doesn't understand, which would otherwise be ignored without a word.
///
/// `tinsel doctor` prints it. Print it from a hidden flag of your own too, so someone whose
/// colours look wrong can tell you why:
///
/// ```no_run
/// if std::env::args().any(|a| a == "--debug-terminal") {
///     tinsel::doctor().print();
///     return;
/// }
/// ```
///
/// Finding the theme can mean asking the terminal, which takes a moment the first time.
pub fn doctor() -> Block {
    let env = |name: &str| std::env::var(name).ok();
    let mut rows: Vec<(&str, String, String)> = Vec::new();

    let colour = match crate::style::color_mode() {
        ColorMode::TrueColor => "truecolor",
        ColorMode::Ansi256 => "256 colours",
        ColorMode::None => "off",
    };
    rows.push(("colour", colour.into(), crate::style::color_why()));

    let theme = match crate::style::theme() {
        Theme::Dark => "dark",
        Theme::Light => "light",
    };
    rows.push(("theme", theme.into(), crate::style::theme_why()));

    let size = format!("{} × {}", crate::term::width(), crate::term::height());
    let measured = if crate::term::measured_size().is_some() {
        "measured".into()
    } else if env("COLUMNS").is_some() || env("LINES").is_some() {
        "from COLUMNS and LINES, with no terminal to measure".into()
    } else {
        "the default, with no terminal to measure".into()
    };
    rows.push(("size", size, measured));

    let output = crate::output::output();
    let shape = match output {
        crate::Output::Pretty => "for people",
        crate::Output::Json => "JSON lines",
        crate::Output::Quiet => "quiet",
    };
    let output_env = env("TINSEL_OUTPUT").filter(|v| valid_output(v));
    let from_env = output_env.as_deref().map(|v| crate::output::parse_output(Some(v))) == Some(output);
    rows.push((
        "output",
        shape.into(),
        source("TINSEL_OUTPUT", from_env, output == crate::Output::Pretty),
    ));

    let target = crate::output::target();
    let stream = match target {
        crate::Target::Stdout => "stdout",
        crate::Target::Stderr => "stderr",
    };
    let target_env = env("TINSEL_TARGET").filter(|v| valid_target(v));
    let from_env = target_env.as_deref().map(|v| crate::output::parse_target(Some(v))) == Some(target);
    rows.push((
        "target",
        stream.into(),
        source("TINSEL_TARGET", from_env, target == crate::Target::Stdout),
    ));

    let accent = crate::style::accent_name();
    let accent_env = env("TINSEL_ACCENT").and_then(|v| Accent::from_name(&v));
    let from_env = accent_env.is_some_and(|a| crate::style::accent_label(a) == accent);
    rows.push((
        "accent",
        accent.clone(),
        source("TINSEL_ACCENT", from_env, accent == "pink"),
    ));

    let fps = crate::fps();
    let fps_env = env("TINSEL_FPS").filter(|v| v.trim().parse::<u32>().is_ok());
    let from_env = fps_env.as_deref().map(|v| crate::fps::parse_fps(Some(v))) == Some(fps);
    rows.push((
        "frame rate",
        format!("{fps} a second"),
        source("TINSEL_FPS", from_env, fps == crate::DEFAULT_FPS),
    ));

    let (focus, why) = crate::watch::why();
    rows.push(("focus", on_off(focus), why));
    let (history, why) = crate::history::why();
    rows.push(("history", on_off(history), why));
    rows.push((
        "notifications",
        crate::notify::how().into(),
        "from TERM and TERM_PROGRAM, when drawing on a terminal".into(),
    ));

    let vars: Vec<(String, String)> = std::env::vars().collect();
    report(&rows, &problems(&vars))
}

/// Laid out as `doctor` shows it: a heading, the rows, then each problem as a warning.
fn report(rows: &[(&str, String, String)], problems: &[String]) -> Block {
    let key_w = rows.iter().map(|(k, _, _)| text::width(k)).max().unwrap_or(0) + 3;
    let value_w = rows.iter().map(|(_, v, _)| text::width(v)).max().unwrap_or(0) + 3;
    let mut block = Block::new();
    block.push(format!(
        "  {}  {}",
        bold(&paint(TEXT, "tinsel")),
        paint(MUTED, env!("CARGO_PKG_VERSION"))
    ));
    block.push("");
    for (key, value, why) in rows {
        block.push(format!(
            "  {}{}{}",
            paint(MUTED, &text::pad(key, key_w)),
            paint(TEXT, &text::pad(value, value_w)),
            paint(SOFT, why)
        ));
    }
    if !problems.is_empty() {
        block.push("");
    }
    for problem in problems {
        block.push(Tone::Warn.line(problem));
    }
    block
}

fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.into()
}

/// What decided a setting: the environment variable, nothing (so the default), or the program.
fn source(var: &str, from_env: bool, is_default: bool) -> String {
    match (from_env, is_default) {
        (true, _) => format!("{var}={}", std::env::var(var).unwrap_or_default()),
        (false, true) => "the default".into(),
        (false, false) => "set in code".into(),
    }
}

fn valid_output(v: &str) -> bool {
    matches!(v, "json" | "quiet" | "pretty")
}

fn valid_target(v: &str) -> bool {
    let v = v.trim();
    v.eq_ignore_ascii_case("stdout") || v.eq_ignore_ascii_case("stderr")
}

/// Settings in `vars` that tinsel can't use as they are, each said as what's wrong and what
/// it would take.
fn problems(vars: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for (name, value) in vars {
        let problem = match name.as_str() {
            "TINSEL_THEME" if !matches!(value.as_str(), "light" | "dark") => {
                format!("TINSEL_THEME={value} is ignored: it can be light or dark")
            }
            "TINSEL_OUTPUT" if !valid_output(value) => {
                format!("TINSEL_OUTPUT={value} is ignored: it can be json, quiet or pretty")
            }
            "TINSEL_TARGET" if !valid_target(value) => {
                format!("TINSEL_TARGET={value} is ignored: it can be stdout or stderr")
            }
            "TINSEL_ACCENT" if Accent::from_name(value).is_none() => format!(
                "TINSEL_ACCENT={value} is ignored: it can be pink, violet, blue, teal, orange or a hue in degrees"
            ),
            "TINSEL_FPS" => match value.trim().parse::<u32>() {
                Err(_) => format!("TINSEL_FPS={value} is ignored: it's a number of frames a second"),
                Ok(n) if !(1..=240).contains(&n) => {
                    format!(
                        "TINSEL_FPS={value} is kept between 1 and 240, so it's {}",
                        n.clamp(1, 240)
                    )
                }
                Ok(_) => continue,
            },
            "TINSEL_NO_FOCUS" | "TINSEL_NO_HISTORY"
                if matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off") =>
            {
                format!("{name}={value} still turns it off: being set is what counts, whatever the value")
            }
            "NO_COLOR" if value.is_empty() => "NO_COLOR is set but empty, which leaves colour on".into(),
            n if n.starts_with("TINSEL_") && !SETTINGS.contains(&n) => match closest(n) {
                Some(near) => format!("{n} isn't a setting tinsel knows: did you mean {near}?"),
                None => format!("{n} isn't a setting tinsel knows"),
            },
            _ => continue,
        };
        found.push(problem);
    }
    found.sort();
    found
}

/// The setting `name` is most likely a misspelling of, if any is close.
fn closest(name: &str) -> Option<&'static str> {
    SETTINGS
        .iter()
        .map(|s| (distance(name, s), *s))
        .filter(|(d, _)| *d <= 3)
        .min()
        .map(|(_, s)| s)
}

/// How many characters to add, remove or change to turn `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let next = (prev + usize::from(ca != cb)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems_with(vars: &[(&str, &str)]) -> Vec<String> {
        let vars: Vec<(String, String)> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        problems(&vars)
    }

    #[test]
    fn says_which_settings_it_cant_use_and_why() {
        let found = problems_with(&[
            ("TINSEL_ACCENT", "pinkish"),
            ("TINSEL_THEME", "grey"),
            ("TINSEL_OUTPUT", "jsn"),
            ("TINSEL_TARGET", "stdrr"),
            ("TINSEL_FPS", "fast"),
            ("TINSEL_NO_FOCUS", "0"),
            ("NO_COLOR", ""),
            ("TINSEL_THEMES", "dark"),
            ("TINSEL_SOMETHING_ELSE", "x"),
            ("HOME", "/home/me"),
        ]);
        let want = [
            "NO_COLOR is set but empty, which leaves colour on",
            "TINSEL_ACCENT=pinkish is ignored: it can be pink, violet, blue, teal, orange or a hue in degrees",
            "TINSEL_FPS=fast is ignored: it's a number of frames a second",
            "TINSEL_NO_FOCUS=0 still turns it off: being set is what counts, whatever the value",
            "TINSEL_OUTPUT=jsn is ignored: it can be json, quiet or pretty",
            "TINSEL_SOMETHING_ELSE isn't a setting tinsel knows",
            "TINSEL_TARGET=stdrr is ignored: it can be stdout or stderr",
            "TINSEL_THEME=grey is ignored: it can be light or dark",
            "TINSEL_THEMES isn't a setting tinsel knows: did you mean TINSEL_THEME?",
        ];
        assert_eq!(found, want);
        assert_eq!(
            problems_with(&[("TINSEL_FPS", "1000")]),
            ["TINSEL_FPS=1000 is kept between 1 and 240, so it's 240"]
        );
    }

    #[test]
    fn settings_it_can_use_pass_quietly() {
        let fine = problems_with(&[
            ("TINSEL_ACCENT", "200"),
            ("TINSEL_THEME", "light"),
            ("TINSEL_OUTPUT", "pretty"),
            ("TINSEL_TARGET", " Stderr "),
            ("TINSEL_FPS", "30"),
            ("TINSEL_NO_HISTORY", "1"),
            ("NO_COLOR", "1"),
        ]);
        assert!(fine.is_empty(), "{fine:?}");
    }

    #[test]
    fn misspellings_are_matched_to_what_was_meant() {
        assert_eq!(closest("TINSEL_ACCENTS"), Some("TINSEL_ACCENT"));
        assert_eq!(closest("TINSEL_NOFOCUS"), Some("TINSEL_NO_FOCUS"));
        assert_eq!(closest("TINSEL_COLOUR"), None);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
    }

    #[test]
    fn the_report_lines_up_and_says_where_each_setting_came_from() {
        crate::set_color_mode(ColorMode::None);
        let rows = [
            ("colour", "off".to_string(), "stdout isn't a terminal".to_string()),
            ("frame rate", "60 a second".to_string(), "the default".to_string()),
        ];
        let shown = report(&rows, &["TINSEL_X isn't a setting tinsel knows".into()]).plain();
        let lines: Vec<&str> = shown.lines().collect();
        assert!(lines[0].starts_with("  tinsel  "), "{shown}");
        assert_eq!(lines[2], "  colour       off           stdout isn't a terminal");
        assert_eq!(lines[3], "  frame rate   60 a second   the default");
        assert_eq!(lines[5], "  ▲  TINSEL_X isn't a setting tinsel knows");
        assert_eq!(source("TINSEL_NO_SUCH_VARIABLE", false, true), "the default");
        assert_eq!(source("TINSEL_NO_SUCH_VARIABLE", false, false), "set in code");
        assert_eq!(on_off(true), "on");
    }

    #[test]
    fn the_whole_report_covers_every_setting() {
        let shown =
            crate::output::with_terminal(false, || crate::style::with_color_mode(ColorMode::None, doctor)).plain();
        for row in [
            "colour",
            "theme",
            "size",
            "output",
            "target",
            "accent",
            "frame rate",
            "focus",
            "history",
            "notifications",
        ] {
            assert!(
                shown.lines().any(|l| l.trim_start().starts_with(row)),
                "{row} in:\n{shown}"
            );
        }
        assert!(shown.contains("chosen for this closure"), "{shown}");
    }
}

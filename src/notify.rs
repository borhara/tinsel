//! Desktop notifications through the terminal, so they come from the window you started in.

enum Kind {
    /// OSC 9: iTerm2, WezTerm, Ghostty, Windows Terminal.
    Osc9,
    /// OSC 99: kitty.
    Kitty,
    /// OSC 777: foot, urxvt, Ghostty.
    Osc777,
    /// macOS Terminal.app has no escape for it, so ask Notification Center directly.
    AppleScript,
    /// Anything else: the bell, which most terminals turn into a badge or a bounce.
    Bell,
}

/// What the terminal understands, going by the environment variables `var` looks up.
fn kind(var: impl Fn(&str) -> Option<String>) -> Kind {
    let program = var("TERM_PROGRAM").unwrap_or_default();
    let term = var("TERM").unwrap_or_default();
    match program.as_str() {
        "iTerm.app" | "WezTerm" | "ghostty" => Kind::Osc9,
        "Apple_Terminal" => Kind::AppleScript,
        _ if term.contains("kitty") || var("KITTY_WINDOW_ID").is_some() => Kind::Kitty,
        _ if var("WT_SESSION").is_some() => Kind::Osc9,
        _ if term.starts_with("foot") || term.contains("rxvt") => Kind::Osc777,
        _ => Kind::Bell,
    }
}

/// Notification Center's messenger; something harmless in tests, which mustn't pop anything up.
const OSASCRIPT: &str = if cfg!(test) { "true" } else { "osascript" };

/// Text that can't break out of an escape sequence or a script string.
fn clean(s: &str) -> String {
    crate::text::strip_ansi(s)
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ';' | '"' | '\\'))
        .collect()
}

/// Send a desktop notification. tinsel's [`Live`](crate::Live) does this for you when a long
/// run finishes while you're looking at another window; call it yourself for anything else.
///
/// Uses whatever the terminal understands (OSC 9, kitty's OSC 99, OSC 777, or Notification
/// Center for macOS Terminal), passed through tmux if needed, and falls back to the bell.
/// Does nothing when stdout (or the [`Target`](crate::Target)) isn't a terminal.
///
/// ```no_run
/// tinsel::notify("backup", "● finished in 12m04s");
/// ```
pub fn notify(title: &str, body: &str) {
    if !crate::output::is_terminal() {
        return;
    }
    let kind = kind(|name| std::env::var(name).ok());
    send(kind, title, body, std::env::var_os("TMUX").is_some());
}

/// How notifications would be sent here, for [`doctor`](crate::doctor).
pub(crate) fn how() -> &'static str {
    match kind(|name| std::env::var(name).ok()) {
        Kind::Osc9 => "OSC 9",
        Kind::Kitty => "kitty's OSC 99",
        Kind::Osc777 => "OSC 777",
        Kind::AppleScript => "Notification Center",
        Kind::Bell => "the bell",
    }
}

fn send(kind: Kind, title: &str, body: &str, tmux: bool) {
    let (title, body) = (clean(title), clean(body));
    let seq = match kind {
        Kind::Osc9 => format!("\x1b]9;{title}: {body}\x07"),
        Kind::Kitty => format!("\x1b]99;;{title}: {body}\x1b\\"),
        Kind::Osc777 => format!("\x1b]777;notify;{title};{body}\x07"),
        Kind::Bell => "\x07".into(),
        Kind::AppleScript => {
            let script = format!("display notification \"{body}\" with title \"{title}\"");
            let _ = std::process::Command::new(OSASCRIPT)
                .args(["-e", &script])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            return;
        }
    };
    // inside tmux, escapes only reach the outer terminal wrapped in a passthrough
    let seq = if tmux {
        format!("\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))
    } else {
        seq
    };
    crate::output::write(&seq);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::{capture, with_terminal};

    fn kind_with(vars: &[(&str, &str)]) -> &'static str {
        let found = kind(|name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()));
        match found {
            Kind::Osc9 => "osc9",
            Kind::Kitty => "kitty",
            Kind::Osc777 => "osc777",
            Kind::AppleScript => "applescript",
            Kind::Bell => "bell",
        }
    }

    #[test]
    fn picks_what_the_terminal_understands() {
        assert_eq!(kind_with(&[("TERM_PROGRAM", "WezTerm")]), "osc9");
        assert_eq!(kind_with(&[("TERM_PROGRAM", "Apple_Terminal")]), "applescript");
        assert_eq!(kind_with(&[("TERM", "xterm-kitty")]), "kitty");
        assert_eq!(kind_with(&[("KITTY_WINDOW_ID", "1")]), "kitty");
        assert_eq!(kind_with(&[("WT_SESSION", "x")]), "osc9");
        assert_eq!(kind_with(&[("TERM", "foot")]), "osc777");
        assert_eq!(kind_with(&[("TERM", "rxvt-unicode")]), "osc777");
        assert_eq!(kind_with(&[("TERM", "xterm")]), "bell");
    }

    #[test]
    fn sends_the_escape_the_terminal_understands() {
        let sent = |kind, tmux| capture(|| send(kind, "build;", "done\x1b[31m\"", tmux));
        assert!(sent(Kind::Osc9, false).contains("\x1b]9;build: done\x07"));
        assert!(sent(Kind::Kitty, false).contains("\x1b]99;;build: done\x1b\\"));
        assert!(sent(Kind::Osc777, false).contains("\x1b]777;notify;build;done\x07"));
        assert!(sent(Kind::Bell, false).contains('\x07'));
        assert!(sent(Kind::Osc9, true).contains("\x1bPtmux;\x1b\x1b]9;build: done\x07\x1b\\"));
        assert!(!sent(Kind::AppleScript, false).contains("build"));
    }

    #[test]
    fn only_notifies_on_a_terminal() {
        assert!(!capture(|| with_terminal(false, || notify("n-title", "body"))).contains("n-title"));
        // whatever this terminal understands, it isn't sent anywhere real
        capture(|| with_terminal(true, || notify("n-title", "body")));
    }
}

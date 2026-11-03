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

fn kind() -> Kind {
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    match program.as_str() {
        "iTerm.app" | "WezTerm" | "ghostty" => Kind::Osc9,
        "Apple_Terminal" => Kind::AppleScript,
        _ if term.contains("kitty") || std::env::var_os("KITTY_WINDOW_ID").is_some() => Kind::Kitty,
        _ if std::env::var_os("WT_SESSION").is_some() => Kind::Osc9,
        _ if term.starts_with("foot") || term.contains("rxvt") => Kind::Osc777,
        _ => Kind::Bell,
    }
}

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
    let (title, body) = (clean(title), clean(body));
    let seq = match kind() {
        Kind::Osc9 => format!("\x1b]9;{title}: {body}\x07"),
        Kind::Kitty => format!("\x1b]99;;{title}: {body}\x1b\\"),
        Kind::Osc777 => format!("\x1b]777;notify;{title};{body}\x07"),
        Kind::Bell => "\x07".into(),
        Kind::AppleScript => {
            let script = format!("display notification \"{body}\" with title \"{title}\"");
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            return;
        }
    };
    // inside tmux, escapes only reach the outer terminal wrapped in a passthrough
    let seq = if std::env::var_os("TMUX").is_some() {
        format!("\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))
    } else {
        seq
    };
    crate::output::write(&seq);
}

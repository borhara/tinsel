//! The system clipboard: copying text to it and pasting from it, by whatever this computer
//! offers.
//!
//! [`copy`] uses the system's own tool where there is one (`pbcopy` on a Mac, `wl-copy` on
//! Wayland, `xclip` or `xsel` on X, PowerShell on Windows and WSL), and otherwise asks the
//! terminal to copy with an OSC 52 escape, which works over ssh too. Over ssh the terminal is
//! asked first, since a tool on the far end would copy to the far end's clipboard.
//! [`paste`] uses the tools alone, as terminals rarely answer a request to read the
//! clipboard, and gives up on one that takes more than a couple of seconds.
//!
//! ```no_run
//! if tinsel::clipboard::copy("https://example.com/build/412") {
//!     tinsel::success("link copied");
//! }
//! let pasted = tinsel::clipboard::paste().unwrap_or_default();
//! ```

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The tools that copy, tried in turn.
const COPY: &[&[&str]] = &[
    &["pbcopy"],
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["xsel", "--clipboard", "--input"],
    // clip.exe reads its input in the console's code page, which garbles anything beyond
    // ASCII, so PowerShell is asked to read it as UTF-8; clip.exe is left for a Windows
    // without it
    &[
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::InputEncoding = [Text.Encoding]::UTF8; Set-Clipboard -Value ([Console]::In.ReadToEnd())",
    ],
    &["clip.exe"],
];

/// The tools that paste, tried in turn.
const PASTE: &[&[&str]] = &[
    &["pbpaste"],
    &["wl-paste", "--no-newline"],
    &["xclip", "-selection", "clipboard", "-o"],
    &["xsel", "--clipboard", "--output"],
    // written as UTF-8, and as it is, without the newline printing it would add
    &[
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; [Console]::Out.Write((Get-Clipboard -Raw))",
    ],
];

/// How long a tool gets to paste: `xclip` can wait forever for a clipboard's owner that
/// never answers.
const PASTE_WAIT: Duration = Duration::from_secs(2);

/// Copy `text` to the clipboard. Gives whether it got there: a tool said it copied, or the
/// terminal was asked to (which it may decline, unseen). False when there's neither a tool
/// nor a terminal.
pub fn copy(text: &str) -> bool {
    let remote = std::env::var_os("SSH_TTY").is_some() || std::env::var_os("SSH_CONNECTION").is_some();
    if remote && ask_terminal(text) {
        return true;
    }
    COPY.iter().any(|tool| run_copy(tool, text)) || (!remote && ask_terminal(text))
}

/// The text on the clipboard, if a tool can read it, with Windows line endings made `\n`.
/// A tool that hasn't answered in two seconds is stopped, and the next one tried.
pub fn paste() -> Option<String> {
    PASTE.iter().find_map(|tool| run_paste(tool, PASTE_WAIT))
}

/// What `tool` prints, if it succeeds within `wait`; it's stopped if it doesn't.
fn run_paste(tool: &[&str], wait: Duration) -> Option<String> {
    let mut child = Command::new(tool[0])
        .args(&tool[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + wait;
    // read on a thread of its own, so a tool that never finishes can be given up on
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = tx.send(stdout.read_to_end(&mut bytes).map(|_| bytes));
    });
    let read = rx.recv_timeout(wait).ok().and_then(Result::ok);
    // its output is all there, but it may not have finished yet
    let mut status = None;
    while read.is_some() && status.is_none() && Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(done)) => status = Some(done),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break,
        }
    }
    let (Some(bytes), Some(status)) = (read, status) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let text = String::from_utf8_lossy(&bytes);
    // in case PowerShell starts its UTF-8 with a byte-order mark
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    status.success().then(|| text.replace("\r\n", "\n"))
}

fn run_copy(tool: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(tool[0])
        .args(&tool[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    // the pipe is closed before waiting, so the tool sees the end of the text
    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|s| s.success()) && wrote
}

/// Send the terminal the escape that copies `text`, if tinsel draws on one.
fn ask_terminal(text: &str) -> bool {
    if !crate::output::is_terminal() {
        return false;
    }
    let mut out = crate::output::writer();
    out.write_all(osc52(text).as_bytes()).is_ok() && out.flush().is_ok()
}

/// The OSC 52 escape asking the terminal to put `text` on the clipboard.
fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// `bytes` in standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        for (text, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
            ("✶ tinsel", "4py2IHRpbnNlbA=="),
        ] {
            assert_eq!(base64(text.as_bytes()), encoded, "{text}");
        }
    }

    #[test]
    fn the_terminal_is_asked_with_osc_52() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
        // nobody to ask off a terminal
        crate::output::with_terminal(false, || assert!(!ask_terminal("hi")));
    }

    #[test]
    fn a_missing_tool_copies_nothing() {
        assert!(!run_copy(&["tinsel-no-such-tool"], "x"));
        assert_eq!(run_paste(&["tinsel-no-such-tool"], PASTE_WAIT), None);
    }

    #[cfg(unix)]
    #[test]
    fn pasting_takes_what_the_tool_prints_and_gives_up_on_one_that_hangs() {
        let printf = ["sh", "-c", "printf 'a\\r\\nb'"];
        assert_eq!(run_paste(&printf, PASTE_WAIT).as_deref(), Some("a\nb"));
        assert_eq!(run_paste(&["sh", "-c", "printf x; exit 1"], PASTE_WAIT), None);

        let started = Instant::now();
        assert_eq!(run_paste(&["sleep", "30"], Duration::from_millis(200)), None);
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
        // and one that's printed everything but doesn't finish
        assert_eq!(
            run_paste(
                &["sh", "-c", "printf x; exec 1>&-; sleep 30"],
                Duration::from_millis(200)
            ),
            None
        );
    }
}

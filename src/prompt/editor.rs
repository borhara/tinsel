//! A longer answer, written in the user's editor.

use super::*;

/// A longer answer written in the user's editor (`$VISUAL`, then `$EDITOR`, then `vi`, or
/// Notepad on Windows), starting from `initial`. Returns what was saved. For commit messages,
/// release notes, anything more than a line.
///
/// ```no_run
/// let notes = tinsel::prompt::editor("Release notes", "## What's new\n\n")?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
///
/// # Errors
///
/// [`Error::NotATerminal`] without a terminal, [`Error::Cancelled`] if the editor exits with an
/// error (`:cq` in vim), and [`Error::Io`] if it can't be started or its file read.
pub fn editor(question: &str, initial: &str) -> Result<String> {
    need_terminal()?;
    let command = editor_command(std::env::var("VISUAL").ok(), std::env::var("EDITOR").ok());
    edit(question, initial, &command)
}

/// The editor to run: `$VISUAL`, then `$EDITOR`, unless they're empty, then the system's own.
pub(super) fn editor_command(visual: Option<String>, editor: Option<String>) -> String {
    let set = |e: &Option<String>| e.as_ref().is_some_and(|e| !e.trim().is_empty());
    let chosen = [visual, editor].into_iter().find(set).flatten();
    chosen.unwrap_or_else(|| if cfg!(windows) { "notepad" } else { "vi" }.into())
}

/// [`editor`], running `command` with the file's path after its own arguments.
pub(super) fn edit(question: &str, initial: &str, command: &str) -> Result<String> {
    let mut words = command.split_whitespace();
    let program = words.next().unwrap_or("vi");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let path = std::env::temp_dir().join(format!("tinsel-{}-{nanos}.md", std::process::id()));
    std::fs::write(&path, initial)?;

    // the editor has the whole terminal: nothing draws over it or reads its keys meanwhile
    let _paused = crate::display::pause_all();
    let mut frame = Frame::default();
    frame.draw(&[format!(
        "{}  {}",
        ask(question),
        paint(MUTED, &format!("opening {program}…"))
    )]);
    let status = std::process::Command::new(program).args(words).arg(&path).status();
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    match status {
        Ok(s) if s.success() => {}
        Ok(_) => return Err(cancel(&mut frame, question)),
        Err(e) => {
            frame.draw(&[format!(
                "{}  {}",
                ask(question),
                paint(RED, &format!("couldn't open {program}"))
            )]);
            return Err(Error::Io(e));
        }
    }
    let text = text?;
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    frame.draw(&[answered(question, &crate::format::plural(lines as u64, "line"))]);
    Ok(text)
}

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
/// [`Error::NotATerminal`] without a terminal (or while a [`script`] answers the questions,
/// since an editor can't be scripted), [`Error::Cancelled`] if the editor exits with an error
/// (`:cq` in vim), and [`Error::Io`] if it can't be started or its file read.
pub fn editor(question: &str, initial: &str) -> Result<String> {
    if scripted() {
        return Err(Error::NotATerminal);
    }
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

/// A file of our own in the temp folder, readable only by us, removed when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    /// A new file holding `text`. Never one that's already there: someone else could have
    /// put it there, or a link to somewhere else.
    fn new(text: &str) -> io::Result<Scratch> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut tries = 0;
        loop {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let name = format!("tinsel-{}-{nanos}-{tries}.md", std::process::id());
            let path = std::env::temp_dir().join(name);
            match options.open(&path) {
                Ok(mut file) => {
                    let scratch = Scratch(path);
                    file.write_all(text.as_bytes())?;
                    return Ok(scratch);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && tries < 100 => tries += 1,
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// How to run `command` on `path`. On Unix it goes to the shell as git does, so an editor
/// can be quoted or have spaces and arguments (`code --wait`); the path is passed as an
/// argument, never put in the command, so nothing in it is run.
fn editor_process(command: &str, path: &std::path::Path) -> std::process::Command {
    #[cfg(unix)]
    {
        let mut run = std::process::Command::new("sh");
        run.arg("-c").arg(format!("{command} \"$@\"")).arg(command).arg(path);
        run
    }
    #[cfg(not(unix))]
    {
        let mut words = command.split_whitespace();
        let mut run = std::process::Command::new(words.next().unwrap_or("notepad"));
        run.args(words).arg(path);
        run
    }
}

/// [`editor`], running `command` with the file's path after its own arguments.
pub(super) fn edit(question: &str, initial: &str, command: &str) -> Result<String> {
    let program = command.split_whitespace().next().unwrap_or("vi");
    let file = Scratch::new(initial)?;

    // the editor has the whole terminal: nothing draws over it or reads its keys meanwhile
    let _paused = crate::display::pause_all();
    let mut frame = Frame::default();
    frame.draw(&[format!(
        "{}  {}",
        ask(question),
        paint(MUTED, &format!("opening {program}…"))
    )]);
    let status = editor_process(command, &file.0).status();
    let text = std::fs::read_to_string(&file.0);
    drop(file);
    let couldnt_open = |frame: &mut Frame| {
        frame.draw(&[format!(
            "{}  {}",
            ask(question),
            paint(RED, &format!("couldn't open {program}"))
        )]);
    };
    match status {
        Ok(s) if s.success() => {}
        // the shell's way of saying it found no such command, or couldn't run it
        Ok(s) if cfg!(unix) && matches!(s.code(), Some(126 | 127)) => {
            couldnt_open(&mut frame);
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("couldn't run the editor “{command}”"),
            )));
        }
        Ok(_) => return Err(cancel(&mut frame, question)),
        Err(e) => {
            couldnt_open(&mut frame);
            return Err(Error::Io(e));
        }
    }
    let text = text?;
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    frame.draw(&[answered(question, &crate::format::plural(lines as u64, "line"))]);
    Ok(text)
}

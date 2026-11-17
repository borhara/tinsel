//! Commands run as steps, with timeouts and retries, and why one failed.

use super::*;

/// How long to wait before retry number `attempt` (from 1).
pub(super) fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt.saturating_sub(1).min(4)))
}

pub(super) fn attempts_note(error: &str, retries: u32) -> String {
    match retries {
        0 => error.to_string(),
        n => {
            let (first, rest) = error.split_once('\n').unwrap_or((error, ""));
            let rest = if rest.is_empty() {
                String::new()
            } else {
                format!("\n{rest}")
            };
            format!("{first} (after {} attempts){rest}", n + 1)
        }
    }
}

/// A command to run as a [`Step`], from [`Step::command`]: set a timeout, retries or what it
/// reads, then [`run`](StepCommand::run) it.
///
/// ```no_run
/// use std::process::Command;
/// use std::time::Duration;
///
/// let live = tinsel::Live::new("deploy", "my-app");
/// live.step("Push image")
///     .command(Command::new("docker").args(["push", "my-app:latest"]))
///     .timeout(Duration::from_secs(120))
///     .retries(2)
///     .run()?;
/// # Ok::<(), tinsel::RunError>(())
/// ```
#[must_use = "the command only runs when `run` is called"]
pub struct StepCommand<'a, 'c> {
    pub(super) step: &'a Step,
    pub(super) command: Box<dyn BorrowMut<Command> + Send + Sync + 'c>,
    pub(super) stdin: Option<Stdio>,
    pub(super) timeout: Option<Duration>,
    pub(super) retries: u32,
    pub(super) tail: usize,
}

impl std::fmt::Debug for StepCommand<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepCommand")
            .field("step", self.step)
            .field("command", (*self.command).borrow())
            .field("timeout", &self.timeout)
            .field("retries", &self.retries)
            .field("tail", &self.tail)
            .finish_non_exhaustive()
    }
}

impl StepCommand<'_, '_> {
    /// Kill the command if it's still running after this long. Each retry gets the full time.
    pub fn timeout(mut self, limit: Duration) -> Self {
        self.timeout = Some(limit);
        self
    }

    /// Run it again, up to `n` more times, if it fails or times out, waiting a little longer
    /// each time. Succeeding after a retry ends the step on a warning.
    pub fn retries(mut self, n: u32) -> Self {
        self.retries = n;
        self
    }

    /// How many of the last lines of output to show under the step if it fails (default 12).
    pub fn tail(mut self, lines: usize) -> Self {
        self.tail = lines;
        self
    }

    /// What the command reads as its input. Nothing unless this says otherwise, so a command
    /// that stops to ask something fails rather than waiting unseen; `Stdio::inherit()` hands
    /// it the terminal's input, for one that reads what's typed. Prompts written straight to
    /// the terminal, as `sudo` and `ssh` ask for passwords, work either way.
    ///
    /// ```no_run
    /// use std::process::{Command, Stdio};
    ///
    /// let live = tinsel::Live::new("setup", "my-app");
    /// live.step("Install").command(Command::new("./install.sh")).stdin(Stdio::inherit()).run()?;
    /// # Ok::<(), tinsel::RunError>(())
    /// ```
    pub fn stdin(mut self, stdin: Stdio) -> Self {
        self.stdin = Some(stdin);
        self
    }

    /// Run the command, marking the step done or failed. Returns everything the command
    /// printed, stdout and stderr together.
    ///
    /// # Errors
    ///
    /// A [`RunError`] if the command can't be started, exits unsuccessfully or runs past its timeout,
    /// on its last attempt.
    pub fn run(mut self) -> Result<String, RunError> {
        self.step.start_if_pending();
        let stdin = self.stdin.take().unwrap_or_else(Stdio::null);
        (*self.command).borrow_mut().stdin(stdin);
        let mut attempt = 0;
        loop {
            match self.once() {
                Ok(output) => {
                    self.step.settle_after(attempt);
                    return Ok(output);
                }
                Err(e) if attempt < self.retries && e.kind.retryable() => {
                    attempt += 1;
                    self.step
                        .detail(format!("attempt {} of {} · {e}", attempt + 1, self.retries + 1));
                    thread::sleep(backoff(attempt));
                }
                Err(e) => {
                    let mut note = e.to_string();
                    let lines: Vec<&str> = e.output.lines().filter(|l| !l.trim().is_empty()).collect();
                    for line in &lines[lines.len().saturating_sub(self.tail)..] {
                        note += "\n";
                        note += line;
                    }
                    self.step.fail(attempts_note(&note, attempt));
                    return Err(e);
                }
            }
        }
    }

    fn once(&mut self) -> Result<String, RunError> {
        let command = (*self.command).borrow_mut();
        let program = command.get_program().to_string_lossy().into_owned();
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunError {
                program: program.clone(),
                output: String::new(),
                kind: RunErrorKind::Spawn(e),
            })?;
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let readers: Vec<JoinHandle<()>> = [
            child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
            child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|pipe| {
            let (lines, step) = (lines.clone(), self.step.clone());
            thread::spawn(move || read_lines(pipe, &lines, &step))
        })
        .collect();

        let started = Instant::now();
        let outcome = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if self.timeout.is_some_and(|t| started.elapsed() >= t) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(RunErrorKind::TimedOut(self.timeout.unwrap_or_default()));
                }
                Ok(None) => thread::sleep(Duration::from_millis(30)),
                Err(e) => break Err(RunErrorKind::Spawn(e)),
            }
        };
        // anything the command started and left running (a background job, or the children
        // of a killed shell) can hold its output open for ever, so stop waiting soon after
        // the command itself has gone
        let grace = Instant::now() + Duration::from_millis(250);
        while readers.iter().any(|r| !r.is_finished()) && Instant::now() < grace {
            thread::sleep(Duration::from_millis(10));
        }
        for reader in readers.into_iter().filter(JoinHandle::is_finished) {
            let _ = reader.join();
        }
        let output = lock(&lines).join("\n");
        let kind = match outcome {
            Ok(status) if status.success() => return Ok(output),
            Ok(status) => RunErrorKind::Exit(status),
            Err(kind) => kind,
        };
        Err(RunError { program, output, kind })
    }
}

/// Collect a command's output line by line, showing the latest as the step's detail. Lines
/// end at `\n` or `\r`, so progress output that redraws itself shows its latest state.
pub(super) fn read_lines(mut pipe: Box<dyn Read + Send>, lines: &Mutex<Vec<String>>, step: &Step) {
    let mut pending = Vec::new();
    let mut buf = [0u8; 4096];
    let finish = |bytes: &mut Vec<u8>| {
        let line = crate::text::strip_ansi(&String::from_utf8_lossy(bytes));
        bytes.clear();
        let line = line.trim_end().to_string();
        if line.trim().is_empty() {
            return;
        }
        step.detail(crate::text::truncate(line.trim(), 60));
        lock(lines).push(line);
    };
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                finish(&mut pending);
            } else {
                pending.push(b);
            }
        }
    }
    finish(&mut pending);
}

/// Why a command run as a step didn't succeed. Shows as `cargo exited with status 101`.
#[derive(Debug)]
pub struct RunError {
    program: String,
    output: String,
    kind: RunErrorKind,
}

/// What went wrong running a command, from [`RunError::kind`].
#[derive(Debug)]
#[non_exhaustive]
pub enum RunErrorKind {
    /// It couldn't be started: not installed, say, or not allowed to run.
    Spawn(std::io::Error),
    /// It ran and failed, ending with this status.
    Exit(ExitStatus),
    /// It was stopped for running past its [`timeout`](StepCommand::timeout), this long.
    TimedOut(Duration),
}

impl RunErrorKind {
    /// A program that isn't there won't appear on a second try.
    fn retryable(&self) -> bool {
        !matches!(self, RunErrorKind::Spawn(_))
    }
}

impl RunError {
    /// The program that was run, as it was named: `cargo`, `./deploy.sh`.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// What went wrong.
    pub fn kind(&self) -> &RunErrorKind {
        &self.kind
    }

    /// The exit code, if the command ran to the end and exited with one (rather than being
    /// stopped by a signal).
    pub fn code(&self) -> Option<i32> {
        self.status().and_then(|s| s.code())
    }

    /// Everything the command printed before it stopped, stdout and stderr together.
    pub fn output(&self) -> &str {
        &self.output
    }

    /// How the command exited, if it ran to the end.
    pub fn status(&self) -> Option<ExitStatus> {
        match self.kind {
            RunErrorKind::Exit(status) => Some(status),
            _ => None,
        }
    }

    /// Whether it was stopped for running past its [`timeout`](StepCommand::timeout).
    pub fn timed_out(&self) -> bool {
        matches!(self.kind, RunErrorKind::TimedOut(_))
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let program = &self.program;
        match &self.kind {
            RunErrorKind::Spawn(e) => write!(f, "couldn't run {program}: {e}"),
            RunErrorKind::Exit(status) => match status.code() {
                Some(code) => write!(f, "{program} exited with status {code}"),
                None => write!(f, "{program} was stopped by a signal"),
            },
            RunErrorKind::TimedOut(limit) => write!(f, "{program} timed out after {}", format::duration(*limit)),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            RunErrorKind::Spawn(e) => Some(e),
            _ => None,
        }
    }
}

// a command can be set up on one thread and run on another, as it could before
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<StepCommand<'static, 'static>>();
};

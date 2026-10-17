use std::io::{IsTerminal, Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::block::Block;
use crate::history::{self, History};
use crate::output::{self, Val};
use crate::style::palette::{AMBER, MUTED, RED, ROSE, SOFT, TEXT};
use crate::style::{Line, Rgb, mix, paint};
use crate::watch::{Focus, Watch};
use crate::widgets::{self, BarStyle, Card, Spinner};
use crate::{format, notify, term};

const MAX_WIDTH: usize = 76;
const FRAME: Duration = Duration::from_millis(33);
/// Redraws slow right down while nobody's looking.
const AWAY_FRAME: Duration = Duration::from_millis(250);
/// Runs shorter than this never send a notification.
const DEFAULT_NOTIFY_AFTER: Duration = Duration::from_secs(10);
/// Steps that usually take less than this don't get an estimate bar.
const MIN_ESTIMATE: f64 = 1.0;
const HIDE_CURSOR: &str = "\x1b[?25l";
const SHOW_CURSOR: &str = "\x1b[?25h";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Pending,
    Active,
    Done,
    Warn,
    Fail,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Active => "running",
            State::Done => "done",
            State::Warn => "warn",
            State::Fail => "fail",
        }
    }
}

/// How a display shows itself, decided once when it starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// Redrawn in place on a terminal.
    Draw,
    /// One plain line per finished step, for logs and pipes.
    Plain,
    /// One JSON event per change.
    Json,
    /// Nothing but failures.
    Quiet,
    /// Nothing at all, on a clock that only moves when told: for tests.
    Headless,
}

struct StepData {
    label: String,
    /// What the step's timings are remembered under: the title and every label down to this one.
    key: String,
    depth: usize,
    children: Vec<usize>,
    state: State,
    started: Option<Instant>,
    ended: Option<Instant>,
    detail: String,
    note: String,
    progress: Option<(f32, String)>,
    /// How long this step usually takes, in seconds, from earlier runs.
    estimate: Option<f64>,
}

struct Shared {
    title: String,
    target: String,
    steps: Vec<StepData>,
    /// Steps with no parent, in the order they were added.
    roots: Vec<usize>,
    footer: Vec<String>,
    /// Lines logged since the last frame, waiting to be printed above the steps. A headless
    /// display keeps them all, so snapshots show them.
    logs: Vec<String>,
    t0: Instant,
    /// A headless display's clock: how far it has been moved on since it started.
    clock: Option<Duration>,
    drawn: usize,
    /// Rows that scrolled off the top of a display taller than the terminal: left in
    /// scrollback as they were, never redrawn.
    committed: usize,
    header_drawn: bool,
    /// A panic took over the screen: stop drawing.
    abandoned: bool,
    mode: Mode,
    spinner: Spinner,
    bar: BarStyle,
    history: Option<History>,
    focus: Option<Arc<Focus>>,
}

impl Shared {
    fn now(&self) -> Instant {
        match self.clock {
            Some(offset) => self.t0 + offset,
            None => Instant::now(),
        }
    }
}

/// A list of steps redrawn in place as they run.
///
/// Steps can run one after another or at the same time: [`Step`] handles are cheap to clone
/// and can be moved to other threads, and every active step animates on its own. Steps can
/// have sub-steps ([`Step::step`]), and [`log`](Live::log) prints a line above the display
/// without disturbing it.
///
/// Beyond spinners and timings, a `Live` display does three things on its own:
///
/// - **It learns.** Each step's duration is remembered between runs (see
///   [`remember_as`](Live::remember_as)). Next time, a step that would only have had a spinner
///   gets a progress bar and an estimate: `usually 42s · 18s left`, or `slower than usual`.
/// - **It notices when you look away.** In terminals that report window focus, animation
///   slows while you're in another window. If a run of 10 seconds or more finishes while
///   you're away, you get a desktop notification ([`notify_after`](Live::notify_after)), and
///   when you come back, the steps that finished meanwhile twinkle once so you can see them.
/// - **It stays out of the way of typing.** Keys pressed during the run don't scribble over
///   the display; they're handed back to the shell when it ends.
///
/// When stdout isn't a terminal it does none of this and prints one plain line per finished
/// step, so CI logs stay readable. With [`Output::Json`](crate::Output::Json) every change is
/// a JSON event instead, and with [`Output::Quiet`](crate::Output::Quiet) only failures print.
pub struct Live {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    watch: Option<Watch>,
    notify_after: Option<Duration>,
    mode: Mode,
    closed: bool,
    prev_interrupt: Option<usize>,
}

/// A handle to one step of a [`Live`] display. Cheap to clone and safe to move to other threads.
#[derive(Clone)]
pub struct Step {
    shared: Arc<Mutex<Shared>>,
    idx: usize,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

fn content_width() -> usize {
    term::width().min(MAX_WIDTH)
}

fn fmt_secs(s: f64) -> String {
    if s < 60.0 {
        format!("{s:.1}s")
    } else {
        format!("{}m{:02}s", (s / 60.0) as u64, (s % 60.0) as u64)
    }
}

/// Whole seconds, rounded up, for estimates that shouldn't flicker every frame.
fn approx(secs: f64) -> String {
    if secs < 60.0 {
        format!("{}s", secs.ceil().max(1.0))
    } else {
        format::duration(Duration::from_secs_f64(secs))
    }
}

/// How far along a step probably is: in step with the clock until 90% of its usual time,
/// then easing towards (but never reaching) the end, so it never looks finished early.
fn estimated_fraction(elapsed: f64, usual: f64) -> f32 {
    let x = elapsed / usual;
    let f = if x < 0.9 {
        x
    } else {
        0.9 + 0.09 * (1.0 - (-(x - 0.9) * 3.0).exp())
    };
    f as f32
}

/// Displays drawing right now, so a panic can stop them and leave a clean screen.
static DRAWING: Mutex<Vec<Weak<Mutex<Shared>>>> = Mutex::new(Vec::new());

/// Stop every display mid-draw: running steps are marked failed, the last frame is drawn,
/// and nothing is drawn after it. Called by the panic hook before it prints. Returns whether
/// there were any.
pub(crate) fn abandon_all() -> bool {
    let drawing = std::mem::take(&mut *DRAWING.lock().unwrap_or_else(|e| e.into_inner()));
    let mut any = false;
    for shared in drawing.iter().filter_map(Weak::upgrade) {
        // the panicking thread might be the one holding the lock, so never wait forever
        let mut s = None;
        for _ in 0..50 {
            match shared.try_lock() {
                Ok(guard) => s = Some(guard),
                Err(TryLockError::Poisoned(e)) => s = Some(e.into_inner()),
                Err(TryLockError::WouldBlock) => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
            }
            break;
        }
        let Some(mut s) = s else { continue };
        let now = s.now();
        for step in s.steps.iter_mut().filter(|st| st.state == State::Active) {
            step.state = State::Fail;
            step.ended = Some(now);
            step.note = "interrupted by a panic".into();
        }
        paint_frame(&mut s, true);
        s.abandoned = true;
        print!("{SHOW_CURSOR}");
        let _ = std::io::stdout().flush();
        any = true;
    }
    any
}

impl Live {
    /// Start a live display headed `title  target`, e.g. `deploy  my-app → production`.
    ///
    /// It starts drawing straight away; add steps with [`Live::step`].
    pub fn new(title: impl Into<String>, target: impl Into<String>) -> Self {
        let mode = match output::output() {
            output::Output::Json => Mode::Json,
            output::Output::Quiet => Mode::Quiet,
            output::Output::Pretty if std::io::stdout().is_terminal() => Mode::Draw,
            output::Output::Pretty => Mode::Plain,
        };
        let mut live = Live::with_mode(title.into(), target.into(), mode);
        match mode {
            Mode::Plain => {
                let s = lock(&live.shared);
                println!("{} {}", s.title, s.target);
            }
            Mode::Json => {
                let s = lock(&live.shared);
                output::emit(
                    "live",
                    &[("title", Val::Str(&s.title)), ("target", Val::Str(&s.target))],
                );
            }
            Mode::Draw => live.start_drawing(),
            Mode::Quiet | Mode::Headless => {}
        }
        live
    }

    /// A display that never draws, on a clock that only moves when you call
    /// [`advance`](Live::advance), for checking what a run looks like in tests. Take a
    /// [`snapshot`](Live::snapshot) whenever you want to see it. It keeps no history and
    /// sends no notifications.
    ///
    /// ```
    /// use std::time::Duration;
    /// use tinsel::Live;
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let live = Live::headless("deploy", "my-app");
    /// let build = live.step("Build");
    /// build.start();
    /// live.advance(Duration::from_millis(1200));
    /// build.done();
    /// assert!(live.snapshot().plain().contains("●   Build"));
    /// assert!(live.snapshot().plain().ends_with("1.2s"));
    /// ```
    pub fn headless(title: impl Into<String>, target: impl Into<String>) -> Self {
        let live = Live::with_mode(title.into(), target.into(), Mode::Headless);
        lock(&live.shared).clock = Some(Duration::ZERO);
        live
    }

    fn with_mode(title: String, target: String, mode: Mode) -> Self {
        Live {
            shared: Arc::new(Mutex::new(Shared {
                title,
                target,
                steps: Vec::new(),
                roots: Vec::new(),
                footer: Vec::new(),
                logs: Vec::new(),
                t0: Instant::now(),
                clock: None,
                drawn: 0,
                committed: 0,
                header_drawn: false,
                abandoned: false,
                mode,
                spinner: Spinner::default(),
                bar: BarStyle::default(),
                history: None,
                focus: None,
            })),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            watch: None,
            notify_after: Some(DEFAULT_NOTIFY_AFTER),
            mode,
            closed: false,
            prev_interrupt: None,
        }
    }

    fn start_drawing(&mut self) {
        // the theme has to be settled before the focus watcher starts reading stdin, because
        // finding it out can mean asking the terminal and reading its reply
        crate::style::theme();
        self.prev_interrupt = term::guard_interrupt();
        self.watch = Watch::start();
        {
            let mut s = lock(&self.shared);
            s.history = History::open(&History::default_app());
            s.focus = self.watch.as_ref().map(|w| w.focus.clone());
        }
        DRAWING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::downgrade(&self.shared));
        print!("{HIDE_CURSOR}");
        let (shared, stop) = (self.shared.clone(), self.stop.clone());
        let focus = self.watch.as_ref().map(|w| w.focus.clone());
        self.thread = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                draw(&shared, false);
                let away = focus.as_ref().is_some_and(|f| f.away());
                thread::sleep(if away { AWAY_FRAME } else { FRAME });
            }
        }));
    }

    /// Add a pending step. Declaring every step up front shows the whole plan from the start.
    pub fn step(&self, label: impl Into<String>) -> Step {
        add_step(&self.shared, None, label.into())
    }

    /// Print a line above the display, which carries on drawing underneath it: for output
    /// that's worth keeping, like a warning from a compiler or a URL. Safe to call from any
    /// thread. In plain output it's an ordinary line; as JSON, a `"log"` event.
    ///
    /// ```no_run
    /// let live = tinsel::Live::new("build", "my-app");
    /// live.log("using cached dependencies from yesterday");
    /// ```
    pub fn log(&self, text: impl AsRef<str>) {
        log(&self.shared, None, text.as_ref());
    }

    /// Choose how running steps animate (default: [`Spinner::Sparkle`]).
    pub fn with_spinner(self, spinner: Spinner) -> Self {
        lock(&self.shared).spinner = spinner;
        self
    }

    /// Choose how progress bars and estimates are drawn (default: [`BarStyle::Line`]).
    ///
    /// ```no_run
    /// use tinsel::{BarStyle, Live};
    ///
    /// let live = Live::new("render", "intro.mp4").with_bar(BarStyle::Waveform);
    /// ```
    pub fn with_bar(self, bar: BarStyle) -> Self {
        lock(&self.shared).bar = bar;
        self
    }

    /// The name step timings are remembered under. Defaults to the program's own name, which
    /// is right unless several programs share a name or one program wants separate histories.
    /// Steps are told apart by this name, the display's title, and their label.
    ///
    /// Timings live in `$XDG_CACHE_HOME/tinsel` (or `~/.cache/tinsel`), one small file per
    /// name. Only runs drawn on a terminal are recorded, and only steps that succeed.
    /// [`timings`](crate::timings) shows what's been learned.
    pub fn remember_as(self, name: &str) -> Self {
        if self.mode == Mode::Draw {
            lock(&self.shared).history = History::open(name);
        }
        self
    }

    /// Don't remember step timings, or show estimates from earlier runs. Setting the
    /// `TINSEL_NO_HISTORY` environment variable does the same for every program.
    pub fn no_history(self) -> Self {
        lock(&self.shared).history = None;
        self
    }

    /// Send a desktop notification when the run finishes, if it took at least this long and
    /// the terminal window wasn't focused at the time (default: 10 seconds).
    pub fn notify_after(mut self, after: Duration) -> Self {
        self.notify_after = Some(after);
        self
    }

    /// Never send a notification when the run finishes.
    pub fn no_notify(mut self) -> Self {
        self.notify_after = None;
        self
    }

    /// How long since the display started, for a closing "done in 10.2s".
    pub fn elapsed(&self) -> Duration {
        let s = lock(&self.shared);
        s.now() - s.t0
    }

    /// Move a [`headless`](Live::headless) display's clock on. Does nothing to a display on
    /// real time.
    pub fn advance(&self, by: Duration) {
        let mut s = lock(&self.shared);
        if let Some(clock) = s.clock.as_mut() {
            *clock += by;
        }
    }

    /// The display as it looks right now, settled (no twinkles mid-way), with the header and
    /// anything logged. Most useful with [`headless`](Live::headless) displays in tests.
    pub fn snapshot(&self) -> Block {
        let s = lock(&self.shared);
        let mut lines = header(&s);
        lines.extend(s.logs.iter().cloned());
        lines.extend(render(&s, s.now(), content_width(), true));
        lines.into()
    }

    /// When each step ran, as bars on one time axis, so you can see which steps overlapped
    /// and where the time went.
    ///
    /// ```text
    ///   Build                 ━━━━━━━━━━━━━━━━━━              2.3s
    ///     linux-arm64         ━━━━━━━━━━━━━━                  1.8s
    ///     linux-x64           ━━━━━━━━━━━━━━━━━━              2.3s
    ///   Upload                                  ━━━━━━━━━━━━  1.6s
    ///                         ──────────────────────────────
    ///                         0s                          3.9s
    /// ```
    pub fn timeline(&self) -> Block {
        timeline(&lock(&self.shared), content_width()).into()
    }

    /// Finish with a [`timeline`](Live::timeline) under the steps.
    pub fn finish_with_timeline(mut self) {
        {
            let mut s = lock(&self.shared);
            let mut footer = vec![String::new()];
            footer.extend(timeline(&s, content_width()));
            s.footer = footer;
        }
        self.close();
    }

    /// Stop drawing and leave the final state on screen. Dropping a `Live` does the same.
    pub fn finish(mut self) {
        self.close();
    }

    /// Finish with a summary [`Card`] under the steps.
    ///
    /// ```
    /// use tinsel::{Card, Live};
    ///
    /// let live = Live::new("deploy", "my-app");
    /// let build = live.step("Build");
    /// build.start();
    /// build.done();
    /// live.finish_with_card(Card::success("deployed").kv("url", "https://my-app.example.com"));
    /// ```
    pub fn finish_with_card(mut self, card: Card) {
        lock(&self.shared).footer = card.lines().into_lines();
        self.close();
    }

    fn close(&mut self) {
        if std::mem::replace(&mut self.closed, true) {
            return;
        }
        match self.mode {
            Mode::Plain => {
                for line in &lock(&self.shared).footer {
                    println!("{line}");
                }
                return;
            }
            Mode::Json => {
                let s = lock(&self.shared);
                let secs = (s.now() - s.t0).as_secs_f64();
                let card: Vec<String> = s.footer.iter().map(|l| crate::text::strip_ansi(l)).collect();
                let fields = [
                    ("title", Val::Str(&s.title)),
                    ("outcome", Val::Str(outcome(&s))),
                    ("secs", Val::Num(secs)),
                    ("card", Val::List(&card)),
                ];
                output::emit("finish", &fields);
                return;
            }
            Mode::Quiet | Mode::Headless => return,
            Mode::Draw => {}
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        // the last frame is drawn settled, so a step that just finished shows ● rather than mid-twinkle
        draw(&self.shared, true);
        DRAWING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|w| w.upgrade().is_some_and(|s| !Arc::ptr_eq(&s, &self.shared)));
        let away = self.watch.as_ref().is_some_and(|w| w.focus.away());
        if let Some(mut watch) = self.watch.take() {
            watch.stop();
        }
        print!("{SHOW_CURSOR}");
        let _ = std::io::stdout().flush();
        term::restore_interrupt(self.prev_interrupt.take());

        let s = lock(&self.shared);
        if away && self.notify_after.is_some_and(|after| s.t0.elapsed() >= after) {
            notify::notify(&format!("{} {}", s.title, s.target), &summary(&s));
        }
        if let Some(history) = &s.history {
            history.save();
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.close();
    }
}

fn add_step(shared: &Arc<Mutex<Shared>>, parent: Option<usize>, label: String) -> Step {
    let mut s = lock(shared);
    let idx = s.steps.len();
    let (key, depth) = match parent {
        Some(p) => (history::key(&s.steps[p].key, &label), s.steps[p].depth + 1),
        None => (history::key(&s.title, &label), 0),
    };
    s.steps.push(StepData {
        label,
        key,
        depth,
        children: Vec::new(),
        state: State::Pending,
        started: None,
        ended: None,
        detail: String::new(),
        note: String::new(),
        progress: None,
        estimate: None,
    });
    match parent {
        Some(p) => s.steps[p].children.push(idx),
        None => s.roots.push(idx),
    }
    drop(s);
    Step {
        shared: shared.clone(),
        idx,
    }
}

fn log(shared: &Mutex<Shared>, step: Option<usize>, text: &str) {
    let mut s = lock(shared);
    let label = step.map(|i| s.steps[i].label.clone());
    match s.mode {
        Mode::Draw | Mode::Headless => {
            let indent = " ".repeat(2 + s.spinner.width() + 2);
            let line = match &label {
                Some(l) => format!("{indent}{}  {}", paint(MUTED, l), paint(SOFT, text)),
                None => format!("{indent}{}", paint(SOFT, text)),
            };
            s.logs.push(line);
        }
        Mode::Plain => match &label {
            Some(l) => println!("    {l}: {text}"),
            None => println!("    {text}"),
        },
        Mode::Json => {
            let step = label.unwrap_or_default();
            output::emit(
                "log",
                &[
                    ("title", Val::Str(&s.title)),
                    ("step", Val::Str(&step)),
                    ("text", Val::Str(text)),
                ],
            );
        }
        Mode::Quiet => {}
    }
}

/// `ok`, `warn` or `fail`: how the run went overall.
fn outcome(s: &Shared) -> &'static str {
    if s.steps.iter().any(|st| st.state == State::Fail) {
        "fail"
    } else if s.steps.iter().any(|st| st.state == State::Warn) {
        "warn"
    } else {
        "ok"
    }
}

/// One line on how the run went, for the notification.
fn summary(s: &Shared) -> String {
    let took = format::duration(s.t0.elapsed());
    if let Some(failed) = s.steps.iter().find(|st| st.state == State::Fail) {
        return format!("✕ {} failed after {took}", failed.label);
    }
    match s.steps.iter().filter(|st| st.state == State::Warn).count() {
        0 => format!("● finished in {took}"),
        n => format!("▲ finished in {took}, with {}", format::plural(n as u64, "warning")),
    }
}

impl Step {
    fn with<R>(&self, f: impl FnOnce(&mut StepData) -> R) -> R {
        f(&mut lock(&self.shared).steps[self.idx])
    }

    /// Add a pending sub-step, drawn indented under this one. Once a step and all its
    /// sub-steps have succeeded, the sub-steps fold away; if any warned or failed, they stay.
    ///
    /// ```
    /// let live = tinsel::Live::new("release", "v2.1.0");
    /// let build = live.step("Build");
    /// let (linux, mac) = (build.step("linux"), build.step("macos"));
    /// build.start();
    /// for target in [&linux, &mac] {
    ///     target.start();
    ///     target.done();
    /// }
    /// build.done();
    /// ```
    pub fn step(&self, label: impl Into<String>) -> Step {
        add_step(&self.shared, Some(self.idx), label.into())
    }

    /// Mark the step as running: its spinner starts and its timer begins. If it has run
    /// before, its usual duration is looked up so it can show an estimate.
    pub fn start(&self) {
        let mut s = lock(&self.shared);
        let now = s.now();
        let estimate = s.history.as_ref().and_then(|h| h.estimate(&s.steps[self.idx].key));
        let step = &mut s.steps[self.idx];
        step.state = State::Active;
        step.started = Some(now);
        step.estimate = estimate;
        if s.mode == Mode::Json {
            emit_step(&s, self.idx, None);
        }
    }

    /// A dim sub-line under the active step, e.g. what it's working on right now.
    pub fn detail(&self, text: impl Into<String>) {
        let text = text.into();
        self.with(|s| s.detail = text);
    }

    /// Show a progress bar under the active step; `text` sits beside the percentage. A step
    /// that reports its own progress doesn't show an estimate.
    pub fn progress(&self, fraction: f32, text: impl Into<String>) {
        let text = text.into();
        self.with(|s| s.progress = Some((fraction.clamp(0.0, 1.0), text)));
    }

    /// Print a line above the display, labelled with this step: see [`Live::log`].
    pub fn log(&self, text: impl AsRef<str>) {
        log(&self.shared, Some(self.idx), text.as_ref());
    }

    /// Mark the step as finished. Its mark twinkles, then settles into `●`.
    pub fn done(&self) {
        self.end(State::Done, String::new());
    }

    /// Mark the step as finished with something worth a look, shown as a note under it.
    pub fn warn(&self, note: impl Into<String>) {
        self.end(State::Warn, note.into());
    }

    /// Mark the step as failed, with the reason shown under it.
    pub fn fail(&self, note: impl Into<String>) {
        self.end(State::Fail, note.into());
    }

    /// Run `f` as this step: started if it hasn't been, then done on `Ok` or failed on `Err`,
    /// with the error as its note.
    ///
    /// ```no_run
    /// # fn load_config() -> Result<(), std::io::Error> { Ok(()) }
    /// let live = tinsel::Live::new("deploy", "my-app");
    /// live.step("Load config").try_run(load_config)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn try_run<T, E: std::fmt::Display>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
        self.start_if_pending();
        let result = f();
        match &result {
            Ok(_) => self.done(),
            Err(e) => self.fail(e.to_string()),
        }
        result
    }

    /// Like [`try_run`](Step::try_run), but on `Err` tries again, up to `retries` more times,
    /// waiting a little longer each time (0.5s, 1s, 2s, … up to 8s). The step's detail shows
    /// which attempt it's on. Succeeding after a retry ends on a warning, so flaky steps
    /// get noticed.
    pub fn retry<T, E: std::fmt::Display>(&self, retries: u32, mut f: impl FnMut() -> Result<T, E>) -> Result<T, E> {
        self.start_if_pending();
        let mut attempt = 0;
        loop {
            match f() {
                Ok(v) => {
                    self.settle_after(attempt);
                    return Ok(v);
                }
                Err(e) if attempt < retries => {
                    attempt += 1;
                    self.detail(format!("attempt {} of {} · {e}", attempt + 1, retries + 1));
                    thread::sleep(backoff(attempt));
                }
                Err(e) => {
                    self.fail(attempts_note(&e.to_string(), attempt));
                    return Err(e);
                }
            }
        }
    }

    /// Run a command as this step, showing its latest line of output as the step's detail.
    /// Done if it exits successfully; otherwise failed, with its last lines of output under
    /// it. Returns everything it printed (stdout and stderr together).
    ///
    /// ```no_run
    /// use std::process::Command;
    ///
    /// let live = tinsel::Live::new("build", "my-app");
    /// let output = live.step("Compile").run(Command::new("cargo").arg("build"))?;
    /// # Ok::<(), tinsel::RunError>(())
    /// ```
    ///
    /// For a timeout or retries, use [`command`](Step::command).
    pub fn run(&self, command: &mut Command) -> Result<String, RunError> {
        self.command(command).run()
    }

    /// A command to run as this step, with options: see [`StepCommand`].
    pub fn command<'c>(&self, command: &'c mut Command) -> StepCommand<'_, 'c> {
        StepCommand {
            step: self,
            command,
            timeout: None,
            retries: 0,
            tail: 12,
        }
    }

    fn start_if_pending(&self) {
        if self.with(|s| s.state == State::Pending) {
            self.start();
        }
    }

    /// Done, or a warning if it took more than one attempt.
    fn settle_after(&self, retries: u32) {
        if retries == 0 {
            self.done();
        } else {
            self.warn(format!("succeeded on attempt {}", retries + 1));
        }
    }

    fn end(&self, state: State, note: String) {
        let mut s = lock(&self.shared);
        let now = s.now();
        let mode = s.mode;
        let step = &mut s.steps[self.idx];
        let started = *step.started.get_or_insert(now);
        step.state = state;
        step.ended = Some(now);
        step.detail.clear();
        step.progress = None;
        step.note = note;
        let secs = (now - started).as_secs_f64();
        let key = step.key.clone();
        // failures stop early, so their times would drag the estimate down
        if state != State::Fail {
            if let Some(history) = s.history.as_mut() {
                history.record(&key, secs);
            }
        }
        match mode {
            Mode::Plain => println!("{}", plain_line(&s.steps[self.idx], secs)),
            Mode::Quiet if state == State::Fail => eprintln!("{}", plain_line(&s.steps[self.idx], secs)),
            Mode::Json => emit_step(&s, self.idx, Some(secs)),
            _ => {}
        }
    }
}

/// `  [ok] Build (1.2s)`, indented under its parent if it has one.
fn plain_line(step: &StepData, secs: f64) -> String {
    let mark = match step.state {
        State::Warn => "warn",
        State::Fail => "FAIL",
        _ => "ok",
    };
    let indent = "  ".repeat(step.depth + 1);
    let mut note = step.note.lines();
    let tail = note.next().map(|first| format!(" - {first}")).unwrap_or_default();
    let mut out = format!("{indent}[{mark}] {} ({}){tail}", step.label, fmt_secs(secs));
    for line in note {
        out += &format!("\n{indent}    | {line}");
    }
    out
}

fn emit_step(s: &Shared, idx: usize, secs: Option<f64>) {
    let step = &s.steps[idx];
    let mut path = vec![step.label.clone()];
    let mut at = idx;
    while let Some(parent) = s.steps.iter().position(|p| p.children.contains(&at)) {
        path.insert(0, s.steps[parent].label.clone());
        at = parent;
    }
    let mut fields = vec![
        ("title", Val::Str(&s.title)),
        ("step", Val::Str(&step.label)),
        ("path", Val::List(&path)),
        ("state", Val::Str(step.state.name())),
    ];
    if let Some(secs) = secs {
        fields.push(("secs", Val::Num(secs)));
    }
    if !step.note.is_empty() {
        fields.push(("note", Val::Str(&step.note)));
    }
    output::emit("step", &fields);
}

/// How long to wait before retry number `attempt` (from 1).
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt.saturating_sub(1).min(4)))
}

fn attempts_note(error: &str, retries: u32) -> String {
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

/// A command to run as a [`Step`], from [`Step::command`]: set a timeout or retries, then
/// [`run`](StepCommand::run) it.
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
pub struct StepCommand<'a, 'c> {
    step: &'a Step,
    command: &'c mut Command,
    timeout: Option<Duration>,
    retries: u32,
    tail: usize,
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

    /// Run the command, marking the step done or failed. Returns everything the command
    /// printed, stdout and stderr together.
    pub fn run(mut self) -> Result<String, RunError> {
        self.step.start_if_pending();
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
        let program = self.command.get_program().to_string_lossy().into_owned();
        let mut child = self
            .command
            .stdin(Stdio::null())
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
        let output = lock_lines(&lines).join("\n");
        let kind = match outcome {
            Ok(status) if status.success() => return Ok(output),
            Ok(status) => RunErrorKind::Exit(status),
            Err(kind) => kind,
        };
        Err(RunError { program, output, kind })
    }
}

fn lock_lines(lines: &Mutex<Vec<String>>) -> MutexGuard<'_, Vec<String>> {
    lines.lock().unwrap_or_else(|e| e.into_inner())
}

/// Collect a command's output line by line, showing the latest as the step's detail. Lines
/// end at `\n` or `\r`, so progress output that redraws itself shows its latest state.
fn read_lines(mut pipe: Box<dyn Read + Send>, lines: &Mutex<Vec<String>>, step: &Step) {
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
        lock_lines(lines).push(line);
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

#[derive(Debug)]
enum RunErrorKind {
    Spawn(std::io::Error),
    Exit(ExitStatus),
    TimedOut(Duration),
}

impl RunErrorKind {
    /// A program that isn't there won't appear on a second try.
    fn retryable(&self) -> bool {
        !matches!(self, RunErrorKind::Spawn(_))
    }
}

impl RunError {
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

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak};
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
    /// Not run, and not going to be.
    Skipped,
    /// Stopped before it could finish, by someone rather than by a failure.
    Cancelled,
    /// Held up until someone (or something) else acts.
    Waiting,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Active => "running",
            State::Done => "done",
            State::Warn => "warn",
            State::Fail => "fail",
            State::Skipped => "skipped",
            State::Cancelled => "cancelled",
            State::Waiting => "waiting",
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

impl Mode {
    /// How a display shows itself, given the output shape and whether it's drawn on a terminal.
    fn pick(out: output::Output, terminal: bool) -> Mode {
        match out {
            output::Output::Json => Mode::Json,
            output::Output::Quiet => Mode::Quiet,
            output::Output::Pretty if terminal => Mode::Draw,
            output::Output::Pretty => Mode::Plain,
        }
    }
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
    /// When it started waiting, while it is.
    waiting_since: Option<Instant>,
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
    /// Frames a second; 0 for the global rate.
    fps: u32,
    history: Option<History>,
    focus: Option<Arc<Focus>>,
    /// Listening for focus reports. Started from the drawing thread after its first frame,
    /// so [`Live::no_focus`] gets in first.
    watch: Option<Watch>,
    wants_focus: bool,
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
/// A step is pending (`○`) until it [starts](Step::start), then ends [done](Step::done) (`●`),
/// with a [warning](Step::warn) (`▲`) or [failed](Step::fail) (`✕`). It can also be
/// [skipped](Step::skip) (`–`), [cancelled](Step::cancel) (`⊘`), or left [waiting](Step::wait)
/// on someone (`◌`), and a finished step can start again.
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
/// It draws on stdout, or stderr after [`set_target`](crate::set_target). When that isn't a
/// terminal it does none of this and prints one plain line per finished
/// step, so CI logs stay readable. With [`Output::Json`](crate::Output::Json) every change is
/// a JSON event instead, and with [`Output::Quiet`](crate::Output::Quiet) only failures print.
pub struct Live {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
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

/// Seconds left at the rate so far, once there's enough to go on: a couple of seconds and a
/// few percent in, so the first guess isn't wild.
fn time_left(elapsed: f64, fraction: f32) -> Option<f64> {
    let p = f64::from(fraction);
    (elapsed >= 2.0 && (0.04..1.0).contains(&p)).then(|| elapsed * (1.0 - p) / p)
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
    let drawing = std::mem::take(&mut *lock(&DRAWING));
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
        output::write(SHOW_CURSOR);
        any = true;
    }
    any
}

impl Live {
    /// Start a live display headed `title  target`, e.g. `deploy  my-app → production`.
    ///
    /// It starts drawing straight away; add steps with [`Live::step`].
    pub fn new(title: impl Into<String>, target: impl Into<String>) -> Self {
        let mode = Mode::pick(output::output(), output::is_terminal());
        let mut live = Live::with_mode(title.into(), target.into(), mode);
        match mode {
            Mode::Plain => {
                let s = lock(&live.shared);
                output::line(&format!("{} {}", s.title, s.target));
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
                fps: 0,
                history: None,
                focus: None,
                watch: None,
                wants_focus: false,
            })),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
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
        {
            let mut s = lock(&self.shared);
            s.history = History::open(&History::default_app());
            s.wants_focus = true;
        }
        lock(&DRAWING).push(Arc::downgrade(&self.shared));
        output::write(HIDE_CURSOR);
        let (shared, stop) = (self.shared.clone(), self.stop.clone());
        self.thread = Some(thread::spawn(move || {
            let mut first = true;
            while !stop.load(Ordering::Relaxed) {
                draw(&shared, false);
                let frame = crate::fps::frame(lock(&shared).fps);
                if std::mem::take(&mut first) {
                    thread::sleep(frame);
                    watch_focus(&shared);
                    continue;
                }
                let away = lock(&shared).focus.as_ref().is_some_and(|f| f.away());
                thread::sleep(if away { AWAY_FRAME.max(frame) } else { frame });
            }
        }));
    }

    /// Draw this many frames a second, rather than [`fps`](crate::fps) for every display.
    ///
    /// ```no_run
    /// let live = tinsel::Live::new("deploy", "my-app").fps(120);
    /// ```
    pub fn fps(self, fps: u32) -> Self {
        lock(&self.shared).fps = fps.clamp(1, 240);
        self
    }

    /// Don't listen for focus reports: no calmer animation while you're away, no notification
    /// when the run finishes, and stdin is left alone, for a tool that hands stdin to
    /// something else. [`set_focus`](crate::set_focus) or the `TINSEL_NO_FOCUS` environment
    /// variable does the same for every display.
    ///
    /// ```no_run
    /// let live = tinsel::Live::new("deploy", "my-app").no_focus();
    /// ```
    pub fn no_focus(self) -> Self {
        let watch = {
            let mut s = lock(&self.shared);
            s.wants_focus = false;
            s.focus = None;
            s.watch.take()
        };
        if let Some(mut watch) = watch {
            watch.stop();
        }
        self
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

    /// Don't remember step timings, or show estimates from earlier runs.
    /// [`set_history`](crate::set_history) or the `TINSEL_NO_HISTORY` environment variable
    /// does the same for every display.
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
                    output::line(line);
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
        let _ = self.thread.take().map(JoinHandle::join);
        // the last frame is drawn settled, so a step that just finished shows ● rather than mid-twinkle
        draw(&self.shared, true);
        lock(&DRAWING).retain(|w| w.upgrade().is_some_and(|s| !Arc::ptr_eq(&s, &self.shared)));
        let watch = lock(&self.shared).watch.take();
        let away = watch.as_ref().is_some_and(|w| w.focus.away());
        if let Some(mut watch) = watch {
            watch.stop();
        }
        output::write(SHOW_CURSOR);
        term::restore_interrupt(self.prev_interrupt.take());

        let mut s = lock(&self.shared);
        if away && self.notify_after.is_some_and(|after| s.t0.elapsed() >= after) {
            notify::notify(&format!("{} {}", s.title, s.target), &summary(&s));
        }
        record_history(&mut s);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.close();
    }
}

/// Start listening for focus reports, unless the display has been told not to.
fn watch_focus(shared: &Mutex<Shared>) {
    // held while the watch starts, so a `no_focus` meanwhile waits and then stops it
    let mut s = lock(shared);
    if !s.wants_focus {
        return;
    }
    let Some(watch) = Watch::start() else { return };
    s.focus = Some(watch.focus.clone());
    s.watch = Some(watch);
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
        waiting_since: None,
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
            Some(l) => output::line(&format!("    {l}: {text}")),
            None => output::line(&format!("    {text}")),
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

/// Remember how long each step that succeeded took, once the run is over, so a step that ran
/// more than once counts only its last go. Failures stop early and skipped or cancelled steps
/// never ran in full, so their times would drag the estimate down.
fn record_history(s: &mut Shared) {
    let Some(mut history) = s.history.take() else { return };
    let runs = s
        .steps
        .iter()
        .filter(|st| matches!(st.state, State::Done | State::Warn))
        .filter_map(|st| Some((&st.key, (st.ended? - st.started?).as_secs_f64())));
    for (key, secs) in runs {
        history.record(key, secs);
    }
    history.save();
    s.history = Some(history);
}

/// `ok`, `warn`, `cancelled` or `fail`: how the run went overall.
fn outcome(s: &Shared) -> &'static str {
    if s.steps.iter().any(|st| st.state == State::Fail) {
        "fail"
    } else if s.steps.iter().any(|st| st.state == State::Cancelled) {
        "cancelled"
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
    if let Some(cancelled) = s.steps.iter().find(|st| st.state == State::Cancelled) {
        return format!("⊘ {} cancelled after {took}", cancelled.label);
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
    ///
    /// A step that has finished, failed or been waiting can start again, like a job that's
    /// retried: its timer starts over, and only its last run is remembered.
    ///
    /// ```
    /// let live = tinsel::Live::new("deploy", "my-app");
    /// let upload = live.step("Upload");
    /// upload.start();
    /// upload.fail("connection reset");
    /// upload.start();
    /// upload.done();
    /// ```
    pub fn start(&self) {
        let mut s = lock(&self.shared);
        let now = s.now();
        let estimate = s.history.as_ref().and_then(|h| h.estimate(&s.steps[self.idx].key));
        let step = &mut s.steps[self.idx];
        step.state = State::Active;
        step.started = Some(now);
        step.ended = None;
        step.waiting_since = None;
        step.note.clear();
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

    /// Mark the step as skipped: it didn't run and won't. The reason, if not empty, shows
    /// under it. Skipped steps aren't trouble, so they fold away with their parent.
    ///
    /// ```
    /// let live = tinsel::Live::new("check", "my-app");
    /// live.step("Lint").skip("no source files changed");
    /// ```
    pub fn skip(&self, reason: impl Into<String>) {
        self.end(State::Skipped, reason.into());
    }

    /// Mark the step as cancelled: stopped, or never started, because someone called it off
    /// rather than because it failed. The reason, if not empty, shows under it.
    ///
    /// ```
    /// let live = tinsel::Live::new("deploy", "my-app");
    /// let upload = live.step("Upload");
    /// upload.start();
    /// upload.cancel("stopped by hand");
    /// ```
    pub fn cancel(&self, reason: impl Into<String>) {
        self.end(State::Cancelled, reason.into());
    }

    /// Mark the step as waiting on someone or something, said under it, with how long it's
    /// been waiting on the right. [`start`](Step::start) it once it can go on. Waiting is
    /// worth noticing, so it prints in quiet output too.
    ///
    /// ```
    /// let live = tinsel::Live::new("deploy", "my-app → production");
    /// let release = live.step("Release");
    /// release.wait("waiting for approval");
    /// // … once someone approves
    /// release.start();
    /// release.done();
    /// ```
    pub fn wait(&self, on: impl Into<String>) {
        let mut s = lock(&self.shared);
        let now = s.now();
        let mode = s.mode;
        let step = &mut s.steps[self.idx];
        step.state = State::Waiting;
        step.waiting_since = Some(now);
        step.ended = None;
        step.detail.clear();
        step.progress = None;
        step.note = on.into();
        match mode {
            Mode::Plain => output::line(&plain_line(step, None)),
            Mode::Quiet => eprintln!("{}", plain_line(step, None)),
            Mode::Json => emit_step(&s, self.idx, None),
            _ => {}
        }
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
        self.ran(result.as_ref().err().map(ToString::to_string));
        result
    }

    /// Done, or failed with the error.
    fn ran(&self, error: Option<String>) {
        match error {
            None => self.done(),
            Some(e) => self.fail(e),
        }
    }

    /// Like [`try_run`](Step::try_run), but on `Err` tries again, up to `retries` more times,
    /// waiting a little longer each time (0.5s, 1s, 2s, … up to 8s). The step's detail shows
    /// which attempt it's on. Succeeding after a retry ends on a warning, so flaky steps
    /// get noticed.
    pub fn retry<T, E: std::fmt::Display>(&self, retries: u32, mut f: impl FnMut() -> Result<T, E>) -> Result<T, E> {
        self.start_if_pending();
        let mut attempt = 0;
        loop {
            let result = f();
            if !self.try_again(result.as_ref().err().map(ToString::to_string), &mut attempt, retries) {
                return result;
            }
        }
    }

    /// After attempt number `attempt` (from 0) ended with `error` or without: whether to try
    /// again, after a pause; if not, the step is settled.
    fn try_again(&self, error: Option<String>, attempt: &mut u32, retries: u32) -> bool {
        match error {
            None => self.settle_after(*attempt),
            Some(e) if *attempt < retries => {
                *attempt += 1;
                self.detail(format!("attempt {} of {} · {e}", *attempt + 1, retries + 1));
                thread::sleep(backoff(*attempt));
                return true;
            }
            Some(e) => self.fail(attempts_note(&e, *attempt)),
        }
        false
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
        // a skipped step never ran, and a cancelled one only took time if it was running
        let ran = match state {
            State::Skipped => false,
            State::Cancelled => step.state == State::Active,
            _ => true,
        };
        let started = if ran {
            Some(*step.started.get_or_insert(now))
        } else {
            step.started = None;
            None
        };
        step.state = state;
        step.ended = Some(now);
        step.waiting_since = None;
        step.detail.clear();
        step.progress = None;
        step.note = note;
        let secs = started.map(|b| (now - b).as_secs_f64());
        match mode {
            Mode::Plain => output::line(&plain_line(&s.steps[self.idx], secs)),
            Mode::Quiet if state == State::Fail => eprintln!("{}", plain_line(&s.steps[self.idx], secs)),
            Mode::Json => emit_step(&s, self.idx, secs),
            _ => {}
        }
    }
}

/// `  [ok] Build (1.2s)`, indented under its parent if it has one.
fn plain_line(step: &StepData, secs: Option<f64>) -> String {
    let mark = match step.state {
        State::Warn => "warn",
        State::Fail => "FAIL",
        State::Skipped => "skip",
        State::Cancelled => "cancelled",
        State::Waiting => "wait",
        _ => "ok",
    };
    let indent = "  ".repeat(step.depth + 1);
    let mut note = step.note.lines();
    let tail = note.next().map(|first| format!(" - {first}")).unwrap_or_default();
    let took = secs.map(|s| format!(" ({})", fmt_secs(s))).unwrap_or_default();
    let mut out = format!("{indent}[{mark}] {}{took}{tail}", step.label);
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

/// When each step ran, as bars on one time axis: which steps overlapped, and where the time
/// went. Steps that never started are listed without a bar.
fn timeline(s: &Shared, width: usize) -> Vec<String> {
    let now = s.now();
    let mut order = Vec::new();
    fn visit(s: &Shared, idx: usize, order: &mut Vec<usize>) {
        order.push(idx);
        for &c in &s.steps[idx].children {
            visit(s, c, order);
        }
    }
    for &r in &s.roots {
        visit(s, r, &mut order);
    }
    let end = s
        .steps
        .iter()
        .filter_map(|st| st.started.map(|b| st.ended.unwrap_or(now).max(b)))
        .max()
        .unwrap_or(now);
    let total = (end - s.t0).as_secs_f64().max(0.001);
    let label = |st: &StepData| format!("{}{}", "  ".repeat(st.depth), st.label);
    let label_w = order
        .iter()
        .map(|&i| crate::text::width(&label(&s.steps[i])))
        .max()
        .unwrap_or(0);
    let time_w = 7;
    let track = width.saturating_sub(2 + label_w + 2 + 2 + time_w).max(10);
    let col = |at: Instant| ((at - s.t0).as_secs_f64() / total * track as f64).round() as usize;

    let mut out = Vec::new();
    for &i in &order {
        let st = &s.steps[i];
        let name = crate::text::pad(&label(st), label_w);
        let Some(began) = st.started else {
            out.push(format!("  {}", paint(MUTED, &name)));
            continue;
        };
        let finished = st.ended.unwrap_or(now);
        let (a, b) = (col(began).min(track - 1), col(finished).min(track));
        let color = match st.state {
            State::Done => ROSE,
            State::Warn => AMBER,
            State::Fail => RED,
            State::Cancelled => MUTED,
            State::Waiting => mix(AMBER, MUTED, 0.35),
            _ => crate::style::palette::BLUSH,
        };
        let bar = "━".repeat(b.saturating_sub(a).max(1));
        let took = fmt_secs((finished - began).as_secs_f64());
        let gap = track.saturating_sub(a + crate::text::width(&bar));
        out.push(format!(
            "  {}  {}{}{}  {}",
            paint(if st.depth > 0 { SOFT } else { TEXT }, &name),
            " ".repeat(a),
            paint(color, &bar),
            " ".repeat(gap),
            paint(MUTED, &format!("{took:>time_w$}"))
        ));
    }
    let axis_pad = " ".repeat(2 + label_w + 2);
    out.push(format!(
        "{axis_pad}{}",
        paint(crate::style::palette::LINE, &"─".repeat(track))
    ));
    let total_label = fmt_secs(total);
    out.push(format!(
        "{axis_pad}{}{}",
        paint(MUTED, "0s"),
        paint(MUTED, &format!("{total_label:>w$}", w = track.saturating_sub(2)))
    ));
    out
}

fn draw(shared: &Mutex<Shared>, settled: bool) {
    paint_frame(&mut lock(shared), settled);
}

/// Redraw the display over its last frame: the header the first time, then anything logged
/// since, then the steps.
fn paint_frame(s: &mut Shared, settled: bool) {
    if s.abandoned {
        return;
    }
    let mut buf = if s.drawn > 0 {
        format!("\x1b[{}F", s.drawn)
    } else {
        String::new()
    };
    let mut kept = Vec::new();
    if !std::mem::replace(&mut s.header_drawn, true) {
        kept = header(s);
    }
    kept.append(&mut s.logs);
    for line in &kept {
        buf += line;
        buf += "\x1b[K\n";
    }
    // no wrapping while drawing steps: an over-long line is cut at the edge rather than
    // taking up two rows and throwing off the next redraw
    buf += "\x1b[?7l";
    let lines = render(s, s.now(), content_width(), settled);
    let rest = &lines[s.committed.min(lines.len())..];
    // the cursor can only move back up as far as the top of the screen, so a display taller
    // than the terminal leaves its first rows in scrollback and redraws only what fits. The
    // last frame is drawn in full, since nothing is redrawn after it.
    let room = term::height().saturating_sub(1).max(1);
    let (commit, tail) = if !settled && rest.len() > room {
        rest.split_at(rest.len() - room)
    } else {
        (&rest[..0], rest)
    };
    for line in commit.iter().chain(tail) {
        buf += line;
        buf += "\x1b[K\n";
    }
    buf += "\x1b[J\x1b[?7h";
    s.drawn = tail.len();
    s.committed += commit.len();
    output::write(&buf);
}

fn header(s: &Shared) -> Vec<String> {
    let header = format!("  {}  {}", paint(ROSE, &s.title), paint(SOFT, &s.target));
    vec![String::new(), header, String::new()]
}

/// What a frame needs to know beyond the step itself.
struct Frame {
    spinner: Spinner,
    bar: BarStyle,
    now: Instant,
    t: f32,
    width: usize,
    settled: bool,
    /// If someone just came back to the window: when they left and when they returned.
    returned: Option<(Instant, Instant)>,
}

/// The steps (and the closing card, if there is one), without the header.
fn render(s: &Shared, now: Instant, width: usize, settled: bool) -> Vec<String> {
    let frame = Frame {
        spinner: s.spinner,
        bar: s.bar,
        now,
        t: (now - s.t0).as_secs_f32(),
        width,
        settled,
        returned: s.focus.as_ref().and_then(|f| f.just_returned(now)),
    };
    let mut lines = Vec::new();
    for &idx in &s.roots {
        walk(s, idx, &frame, &mut lines);
    }
    lines.extend(s.footer.iter().cloned());
    lines
}

/// A step, then its sub-steps unless they've all quietly succeeded.
fn walk(s: &Shared, idx: usize, frame: &Frame, out: &mut Vec<String>) {
    let step = &s.steps[idx];
    step_lines(step, frame, out);
    let folded = matches!(step.state, State::Done | State::Skipped) && step.children.iter().all(|&c| !troubled(s, c));
    if !folded {
        for &child in &step.children {
            walk(s, child, frame, out);
        }
    }
}

/// Whether a step or anything under it warned, failed, was cancelled or is waiting.
fn troubled(s: &Shared, idx: usize) -> bool {
    let step = &s.steps[idx];
    matches!(
        step.state,
        State::Warn | State::Fail | State::Cancelled | State::Waiting
    ) || step.children.iter().any(|&c| troubled(s, c))
}

fn step_lines(st: &StepData, f: &Frame, out: &mut Vec<String>) {
    // marks sit centred in the spinner's column, so labels stay put when a step finishes;
    // sub-steps start where their parent's label does
    let gutter = f.spinner.width();
    let nest = st.depth * (gutter + 2);
    let indent = " ".repeat(2 + nest + gutter + 2);
    let took = st
        .started
        .map(|b| fmt_secs((st.ended.unwrap_or(f.now) - b).as_secs_f64()))
        .unwrap_or_default();
    let row_with = |glyph: Line, label: Line, right: &str, right_color: Rgb| {
        let left = Line::new().space(2 + nest).append(glyph).space(2).append(label);
        let pad = f.width.saturating_sub(left.width() + right.chars().count() + 2).max(1);
        format!("{left}{}{}", " ".repeat(pad), paint(right_color, right))
    };
    let row = |glyph: Line, label: Line, right_color: Rgb| row_with(glyph, label, &took, right_color);
    let centred = |mark: Line| {
        Line::new()
            .space((gutter - 1) / 2)
            .append(mark)
            .space(gutter - 1 - (gutter - 1) / 2)
    };
    let plain = |glyph: &str, glyph_color: Rgb, label_color: Rgb| {
        (
            centred(Line::new().push(glyph, glyph_color)),
            Line::new().push(&st.label, label_color),
        )
    };
    // a note's first line says what happened; any more (a failed command's last output) are quieter
    let note = |color: Rgb, out: &mut Vec<String>| {
        for (i, line) in st.note.lines().enumerate() {
            out.push(format!("{indent}{}", paint(if i == 0 { color } else { MUTED }, line)));
        }
    };

    match st.state {
        State::Pending => {
            let (g, l) = plain("○", MUTED, MUTED);
            out.push(row(g, l, MUTED));
        }
        State::Active => {
            out.push(row(f.spinner.frame(f.t), widgets::shimmer(&st.label, f.t), MUTED));
            let usual = st.estimate.filter(|e| *e >= MIN_ESTIMATE);
            if let Some((p, text)) = &st.progress {
                let mut stats = paint(SOFT, &format!("{:>3.0}%  {text}", p * 100.0));
                if let Some(left) = st.started.and_then(|b| time_left((f.now - b).as_secs_f64(), *p)) {
                    stats += &paint(MUTED, &format!(" · {} left", approx(left)));
                }
                out.push(format!("{indent}{}  {stats}", f.bar.draw(*p, 30, f.t)));
            } else if let (Some(usual), Some(started)) = (usual, st.started) {
                let elapsed = (f.now - started).as_secs_f64();
                let (text, color) = if elapsed <= usual {
                    (
                        format!("usually {} · {} left", approx(usual), approx(usual - elapsed)),
                        SOFT,
                    )
                } else if elapsed <= usual * 1.25 {
                    (format!("usually {} · nearly there", approx(usual)), SOFT)
                } else {
                    (
                        format!("slower than usual · usually {}", approx(usual)),
                        mix(AMBER, SOFT, 0.35),
                    )
                };
                let bar = f.bar.draw(estimated_fraction(elapsed, usual), 30, f.t);
                out.push(format!("{indent}{bar}  {}", paint(color, &text)));
            }
            if st.progress.is_none() && !st.detail.is_empty() {
                out.push(format!("{indent}{}", paint(MUTED, &st.detail)));
            }
        }
        State::Done => {
            let mut age = st.ended.map_or(f32::MAX, |e| (f.now - e).as_secs_f32());
            // finished while you were away? twinkle again now you're back
            if let (Some((left, back)), Some(ended)) = (f.returned, st.ended) {
                if ended >= left && ended <= back {
                    age = (f.now - back).as_secs_f32();
                }
            }
            let mark = widgets::pop(age)
                .filter(|_| !f.settled)
                .unwrap_or_else(|| Line::new().push("●", ROSE));
            out.push(row(centred(mark), Line::new().push(&st.label, TEXT), MUTED));
        }
        State::Warn => {
            let (g, l) = plain("▲", AMBER, TEXT);
            out.push(row(g, l, MUTED));
            note(mix(AMBER, MUTED, 0.35), out);
        }
        State::Fail => {
            let (g, l) = plain("✕", RED, RED);
            out.push(row(g, l, RED));
            note(mix(RED, SOFT, 0.3), out);
        }
        State::Skipped => {
            let (g, l) = plain("–", MUTED, MUTED);
            out.push(row_with(g, l, "skipped", MUTED));
            note(MUTED, out);
        }
        State::Cancelled => {
            let (g, l) = plain("⊘", SOFT, SOFT);
            let right = if st.started.is_some() {
                took.as_str()
            } else {
                "cancelled"
            };
            out.push(row_with(g, l, right, MUTED));
            note(MUTED, out);
        }
        State::Waiting => {
            // a slow breath, so it reads as patient rather than busy
            let glow = if f.settled {
                AMBER
            } else {
                mix(AMBER, MUTED, 0.5 - 0.5 * (f.t * 2.4).cos())
            };
            let (g, l) = plain("◌", glow, TEXT);
            let waited = st
                .waiting_since
                .map(|w| fmt_secs((f.now - w).as_secs_f64()))
                .unwrap_or_default();
            out.push(row_with(g, l, &waited, MUTED));
            note(mix(AMBER, MUTED, 0.35), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_never_look_finished_early() {
        assert!((estimated_fraction(21.0, 42.0) - 0.5).abs() < 1e-6);
        assert!(estimated_fraction(42.0, 42.0) < 0.95);
        assert!(estimated_fraction(1000.0, 42.0) < 0.991);
        // and keep creeping forward
        assert!(estimated_fraction(60.0, 42.0) > estimated_fraction(50.0, 42.0));
    }

    #[test]
    fn history_keeps_one_time_per_step_and_only_for_steps_that_ran() {
        let live = Live::headless("deploy", "my-app");
        lock(&live.shared).history = Some(History::scratch());
        let (upload, lint, ship) = (live.step("Upload"), live.step("Lint"), live.step("Ship"));
        upload.start();
        live.advance(Duration::from_secs(2));
        upload.done();
        upload.start();
        live.advance(Duration::from_secs(3));
        upload.done();
        lint.skip("");
        ship.start();
        live.advance(Duration::from_secs(1));
        ship.cancel("");
        let mut s = lock(&live.shared);
        record_history(&mut s);
        let history = s.history.as_ref().unwrap();
        assert_eq!(history.runs("deploy › Upload"), [3.0]);
        assert!(history.runs("deploy › Lint").is_empty());
        assert!(history.runs("deploy › Ship").is_empty());
    }

    #[test]
    fn only_a_terminal_gets_a_drawn_display() {
        use output::Output;
        assert_eq!(Mode::pick(Output::Pretty, true), Mode::Draw);
        assert_eq!(Mode::pick(Output::Pretty, false), Mode::Plain);
        assert_eq!(Mode::pick(Output::Json, true), Mode::Json);
        assert_eq!(Mode::pick(Output::Quiet, true), Mode::Quiet);
    }

    #[test]
    fn approximations_round_up_to_whole_seconds() {
        assert_eq!(approx(41.2), "42s");
        assert_eq!(approx(0.3), "1s");
        assert_eq!(approx(187.0), "3m07s");
    }

    use crate::output::{Output, capture, with_output, with_terminal};

    fn plain() {
        crate::set_color_mode(crate::ColorMode::None);
    }

    /// Every way a step can end, on one display.
    fn run_every_state(live: &Live) {
        let (a, b, c, d, e, f, g) = (
            live.step("s-done"),
            live.step("s-warn"),
            live.step("s-fail"),
            live.step("s-skip"),
            live.step("s-cancel"),
            live.step("s-wait"),
            live.step("s-sub-parent"),
        );
        let sub = g.step("s-sub");
        a.start();
        a.log("step-log");
        a.done();
        b.start();
        b.warn("careful");
        c.start();
        c.fail("broke\nsecond line");
        d.skip("not needed");
        e.cancel("stopped");
        f.wait("approval");
        g.start();
        sub.start();
        sub.done();
        g.done();
        live.log("live-log");
    }

    #[test]
    fn draws_on_a_terminal_and_leaves_the_last_frame() {
        plain();
        let out = capture(|| {
            with_terminal(true, || {
                let live = Live::new("d-title", "target")
                    .no_history()
                    .no_focus()
                    .fps(240)
                    .with_spinner(Spinner::Dots);
                run_every_state(&live);
                std::thread::sleep(Duration::from_millis(30));
                live.finish_with_timeline();
                let live = Live::new("d-card", "target")
                    .remember_as("tinsel-unit-test")
                    .no_history()
                    .no_focus();
                live.step("one").done();
                live.finish_with_card(Card::success("card-line"));
            })
        });
        assert!(out.starts_with(HIDE_CURSOR) && out.contains(SHOW_CURSOR), "{out:?}");
        for want in [
            "d-title",
            "s-done",
            "careful",
            "second line",
            "live-log",
            "step-log",
            "0s",
            "card-line",
        ] {
            assert!(out.contains(want), "{want} in {out:?}");
        }
    }

    #[test]
    fn a_tall_display_redraws_only_what_fits() {
        plain();
        let out = capture(|| {
            with_terminal(true, || {
                let live = Live::new("tall", "x").no_history().no_focus().fps(240);
                let steps: Vec<Step> = (0..crate::term::height() + 5)
                    .map(|i| live.step(format!("t{i}")))
                    .collect();
                for s in &steps {
                    s.start();
                }
                std::thread::sleep(Duration::from_millis(40));
                live.finish();
            })
        });
        assert!(out.contains(&format!("t{}", crate::term::height() + 4)));
    }

    #[test]
    fn off_a_terminal_each_step_is_a_line() {
        plain();
        let out = capture(|| {
            with_terminal(false, || {
                let live = Live::new("p-title", "target");
                run_every_state(&live);
                live.finish_with_card(Card::success("p-card"));
            })
        });
        for want in [
            "p-title target\n",
            "  [ok] s-done (",
            "  [warn] s-warn (0.0s) - careful\n",
            "  [FAIL] s-fail (0.0s) - broke\n      | second line\n",
            "  [skip] s-skip - not needed\n",
            "  [cancelled] s-cancel - stopped\n",
            "  [wait] s-wait - approval\n",
            "    [ok] s-sub (",
            "    s-done: step-log\n",
            "    live-log\n",
            "p-card",
        ] {
            assert!(out.contains(want), "{want:?} in {out:?}");
        }
    }

    #[test]
    fn as_json_every_change_is_an_event() {
        let out = capture(|| {
            with_output(Output::Json, || {
                let live = Live::new("j-title", "target");
                run_every_state(&live);
                live.finish_with_card(Card::success("j-card"));
            })
        });
        assert!(
            out.contains(r#"{"event":"live","title":"j-title","target":"target"}"#),
            "{out}"
        );
        for state in ["running", "done", "warn", "fail", "skipped", "cancelled", "waiting"] {
            assert!(out.contains(&format!(r#""state":"{state}""#)), "{state} in {out}");
        }
        assert!(out.contains(r#""path":["s-sub-parent","s-sub"]"#), "{out}");
        assert!(
            out.contains(r#"{"event":"log","title":"j-title","step":"s-done","text":"step-log"}"#),
            "{out}"
        );
        assert!(out.contains(r#""step":"","text":"live-log""#), "{out}");
        assert!(
            out.contains(r#"{"event":"finish","title":"j-title","outcome":"fail","#),
            "{out}"
        );
    }

    #[test]
    fn quietly_only_failures_and_waits_print() {
        let out = capture(|| {
            with_output(Output::Quiet, || {
                let live = Live::new("q-title", "target");
                run_every_state(&live);
            })
        });
        assert!(!out.contains("q-title") && !out.contains("s-done"), "{out}");
    }

    #[test]
    fn a_panic_marks_running_steps_and_stops_drawing() {
        plain();
        capture(|| {
            let make = || with_terminal(true, || Live::new("a-title", "x").no_history().no_focus());
            let (busy, poisoned) = (make(), make());
            busy.step("a-running").start();
            let state = poisoned.shared.clone();
            let _ = std::thread::spawn(move || {
                let _held = state.lock().unwrap();
                panic!("poison the display");
            })
            .join();
            let held = busy.shared.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let holder = std::thread::spawn(move || {
                let _held = held.lock().unwrap();
                tx.send(()).unwrap();
                thread::sleep(Duration::from_millis(10));
            });
            rx.recv().unwrap();
            assert!(abandon_all());
            holder.join().unwrap();
            let s = lock(&busy.shared);
            assert!(s.abandoned && s.steps[0].state == State::Fail, "{}", s.steps[0].note);
            drop(s);
            assert!(!abandon_all());
        });
    }

    #[test]
    fn a_drawn_run_remembers_its_timings() {
        let dir = std::env::temp_dir().join(format!("tinsel-live-{}", std::process::id()));
        capture(|| {
            let live = with_terminal(true, || Live::new("h-title", "x").no_focus());
            lock(&live.shared).history = History::open_in(&dir, "app");
            live.step("Build").done();
            live.finish();
        });
        assert!(
            std::fs::read_to_string(dir.join("app.tsv"))
                .unwrap()
                .starts_with("h-title › Build\t")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn focus_is_only_watched_when_wanted() {
        let live = Live::headless("f", "x");
        watch_focus(&live.shared);
        lock(&live.shared).wants_focus = true;
        // not drawing on a terminal, even when the tests run on one
        with_terminal(false, || watch_focus(&live.shared));
        assert!(lock(&live.shared).watch.is_none(), "no terminal to watch");
    }

    #[test]
    fn the_summary_says_how_it_went() {
        let live = Live::headless("s", "x");
        let (a, b) = (live.step("a"), live.step("b"));
        let summary = || summary(&lock(&live.shared));
        assert!(summary().starts_with("● finished in"));
        a.warn("");
        b.warn("");
        assert!(summary().ends_with("with 2 warnings"), "{}", summary());
        b.cancel("");
        assert!(summary().starts_with("⊘ b cancelled after"));
        a.fail("");
        assert!(summary().starts_with("✕ a failed after"));
        assert_eq!(outcome(&lock(&live.shared)), "fail");
    }

    #[test]
    fn retries_wait_longer_each_time_and_say_how_many_it_took() {
        assert_eq!(backoff(1), Duration::from_millis(500));
        assert_eq!(backoff(9), Duration::from_secs(8));
        assert_eq!(attempts_note("nope", 0), "nope");
        assert_eq!(attempts_note("nope\nmore", 2), "nope (after 3 attempts)\nmore");
        assert_eq!(fmt_secs(75.0), "1m15s");
    }

    #[test]
    fn every_state_has_a_name() {
        let names = [State::Pending, State::Active, State::Waiting].map(State::name);
        assert_eq!(names, ["pending", "running", "waiting"]);
    }

    #[test]
    fn options_that_change_nothing_on_screen_still_take() {
        let live = Live::headless("o", "x")
            .with_bar(BarStyle::Waveform)
            .notify_after(Duration::from_secs(1))
            .remember_as("ignored when headless");
        assert_eq!(live.notify_after, Some(Duration::from_secs(1)));
        assert!(live.no_notify().notify_after.is_none());
        let real = with_output(Output::Quiet, || Live::new("o", "x"));
        real.advance(Duration::from_secs(60));
        assert!(real.elapsed() < Duration::from_secs(60));
    }

    #[test]
    fn the_outcome_is_the_worst_of_the_steps() {
        let live = Live::headless("o", "x");
        let (a, b, c) = (live.step("a"), live.step("b"), live.step("c"));
        let outcome = || outcome(&lock(&live.shared));
        assert_eq!(outcome(), "ok");
        a.warn("");
        assert_eq!(outcome(), "warn");
        b.cancel("");
        assert_eq!(outcome(), "cancelled");
        c.fail("");
        assert_eq!(outcome(), "fail");
    }

    #[test]
    fn closures_run_as_steps() {
        let live = Live::headless("r", "x");
        assert_eq!(live.step("ok").try_run(|| Ok::<_, String>(1)), Ok(1));
        let started = live.step("started");
        started.start();
        assert_eq!(started.try_run(|| Err::<(), _>("late")), Err("late"));
        let mut tries = 0;
        let flaky = live.step("flaky").retry(1, || {
            tries += 1;
            if tries < 2 { Err("not yet") } else { Ok(tries) }
        });
        assert_eq!(flaky, Ok(2));
        let gave_up: Result<(), _> = live.step("gave up").retry(0, || Err("never"));
        assert_eq!(gave_up, Err("never"));
        let s = lock(&live.shared);
        let states: Vec<State> = s.steps.iter().map(|st| st.state).collect();
        assert_eq!(states, [State::Done, State::Fail, State::Warn, State::Fail]);
        assert_eq!(s.steps[2].note, "succeeded on attempt 2");
    }

    #[test]
    fn a_command_that_isnt_there_isnt_retried() {
        let live = Live::headless("c", "x");
        let err = live
            .step("missing")
            .command(&mut Command::new("/nonexistent/tinsel-command"))
            .retries(3)
            .run()
            .unwrap_err();
        assert!(
            err.to_string()
                .starts_with("couldn't run /nonexistent/tinsel-command: "),
            "{err}"
        );
        assert!(std::error::Error::source(&err).is_some());
        assert!(err.status().is_none() && !err.timed_out() && err.output().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn commands_time_out_and_are_tried_again() {
        let live = Live::headless("c", "x");
        let step = live.step("slow");
        let err = step
            .command(Command::new("sleep").arg("5"))
            .timeout(Duration::from_millis(50))
            .retries(1)
            .run()
            .unwrap_err();
        assert!(err.timed_out() && err.status().is_none(), "{err}");
        assert_eq!(err.to_string(), "sleep timed out after 50ms");
        assert!(std::error::Error::source(&err).is_none());
        assert!(lock(&live.shared).steps[0].note.ends_with("(after 2 attempts)"));
    }

    #[cfg(unix)]
    #[test]
    fn commands_say_how_they_ended() {
        let live = Live::headless("c", "x");
        let code = live
            .step("code")
            .run(Command::new("sh").args(["-c", "exit 3"]))
            .unwrap_err();
        assert_eq!(
            (code.to_string().as_str(), code.status().and_then(|s| s.code())),
            ("sh exited with status 3", Some(3))
        );
        let killed = live
            .step("killed")
            .run(Command::new("sh").args(["-c", "kill -9 $$"]))
            .unwrap_err();
        assert_eq!(killed.to_string(), "sh was stopped by a signal");
        // a job left running in the background keeps the output open; it's let go soon after
        let started = Instant::now();
        let bg = live
            .step("bg")
            .run(Command::new("sh").args(["-c", "sleep 2 & echo started"]));
        assert_eq!(bg.unwrap(), "started");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn estimates_say_how_long_is_usual() {
        plain();
        let live = Live::headless("e", "x");
        let mut history = History::scratch();
        for secs in [10.0, 10.0] {
            history.record("e › build", secs);
        }
        lock(&live.shared).history = Some(history);
        let build = live.step("build");
        build.start();
        build.detail("compiling");
        let shown = |live: &Live, by: u64| {
            live.advance(Duration::from_secs(by));
            live.snapshot().plain()
        };
        let early = shown(&live, 4);
        assert!(
            early.contains("usually 10s · 6s left") && early.contains("compiling"),
            "{early}"
        );
        assert!(shown(&live, 7).contains("usually 10s · nearly there"));
        assert!(shown(&live, 5).contains("slower than usual · usually 10s"));
    }

    #[test]
    fn steps_that_finished_while_you_were_away_twinkle_when_you_come_back() {
        let live = Live::headless("t", "x");
        let focus = Arc::new(Focus::default());
        lock(&live.shared).focus = Some(focus.clone());
        let t0 = lock(&live.shared).t0;
        let before = live.step("before leaving");
        before.done();
        live.advance(Duration::from_millis(10));
        focus.focus_changed(false, t0 + Duration::from_millis(10));
        let step = live.step("while away");
        step.start();
        live.advance(Duration::from_secs(5));
        step.done();
        let back = t0 + Duration::from_secs(6);
        focus.focus_changed(true, back);
        live.advance(Duration::from_millis(1100));
        let s = lock(&live.shared);
        let twinkling = render(&s, back + Duration::from_millis(100), 60, false).join("\n");
        let settled = render(&s, back + Duration::from_millis(100), 60, true).join("\n");
        assert_ne!(twinkling, settled);
    }

    #[test]
    fn the_timeline_colours_every_state() {
        plain();
        let live = Live::headless("tl", "x");
        let (a, b, c) = (live.step("cancelled"), live.step("waiting"), live.step("running"));
        a.start();
        live.advance(Duration::from_secs(1));
        a.cancel("");
        b.start();
        b.wait("someone");
        c.start();
        live.advance(Duration::from_secs(1));
        let timeline = live.timeline().plain();
        assert!(timeline.matches('━').count() > 2, "{timeline}");
    }

    #[test]
    fn sub_steps_of_sub_steps_fold_away_too() {
        plain();
        let live = Live::headless("n", "x");
        let (top, middle) = (live.step("top"), live.step("other"));
        let child = top.step("child");
        let grandchild = child.step("grandchild");
        for s in [&grandchild, &child, &top, &middle] {
            s.done();
        }
        assert!(!live.snapshot().plain().contains("grandchild"));
    }

    #[test]
    fn progress_shows_how_far_and_how_long_is_left() {
        plain();
        let live = Live::headless("p", "x");
        let (step, gone) = (live.step("upload"), live.step("gone"));
        step.start();
        step.progress(0.01, "just started");
        assert!(!live.snapshot().plain().contains("left"), "too soon to say");
        live.advance(Duration::from_secs(10));
        step.progress(0.5, "17 / 34 MB");
        gone.start();
        live.advance(Duration::from_secs(2));
        gone.cancel("");
        let shown = live.snapshot().plain();
        assert!(shown.contains(" 50%  17 / 34 MB") && shown.contains("left"), "{shown}");
        assert!(shown.contains("2.0s"), "{shown}");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_command_shows_its_last_lines() {
        let live = Live::headless("c", "x");
        let err = live
            .step("noisy")
            .command(Command::new("sh").args(["-c", "echo one; echo two; exit 1"]))
            .tail(1)
            .run();
        assert!(err.is_err());
        assert_eq!(lock(&live.shared).steps[0].note, "sh exited with status 1\ntwo");
    }
}

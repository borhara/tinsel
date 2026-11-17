use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::block::Block;
use crate::display::{self, Mode, lock, lock_patiently};
use crate::history::{self, History};
use crate::output::{self, Val};
use crate::style::palette::{AMBER, MUTED, RED, ROSE, SOFT, TEXT};
use crate::style::{Line, Rgb, mix, paint};
use crate::watch::{Focus, Watch};
use crate::widgets::{self, BarStyle, Card, Spinner};
use crate::{format, notify, term};

mod command;
mod render;
#[cfg(test)]
mod tests;
mod timeline;

pub use self::command::*;
use self::{render::*, timeline::*};

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
    /// Questions open over this display ([`display::pause_all`]): nothing is drawn while any is.
    paused: u32,
    /// The focus watch was stopped for a question: start it again on the next frame.
    refocus: bool,
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
///   the display; they're handed back to the shell when it ends, where the system allows it.
///   Linux 6.2 and later doesn't by default (`dev.tty.legacy_tiocsti`), so there they're lost.
///
/// It draws on stdout, or stderr after [`set_target`](crate::set_target). When that isn't a
/// terminal it does none of this and prints one plain line per finished
/// step, so CI logs stay readable. With [`Output::Json`](crate::Output::Json) every change is
/// a JSON event instead, and with [`Output::Quiet`](crate::Output::Quiet) only failures print.
pub struct Live {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    /// Set once the display has started: its header shown, and drawing begun.
    started: Once,
    thread: Mutex<Option<JoinHandle<()>>>,
    notify_after: Option<Duration>,
    /// The name step timings are kept under: the program's own unless chosen, none if off.
    history: Option<String>,
    mode: Mode,
    closed: bool,
    prev_interrupt: Mutex<Option<usize>>,
}

/// A handle to one step of a [`Live`] display. Cheap to clone and safe to move to other threads.
#[derive(Clone)]
pub struct Step {
    shared: Arc<Mutex<Shared>>,
    idx: usize,
}

fn content_width() -> usize {
    term::width().min(MAX_WIDTH)
}

/// Seconds as [`format::duration`] says them, so a step reads the same as a spinner's line.
fn fmt_secs(s: f64) -> String {
    format::duration(Duration::try_from_secs_f64(s).unwrap_or_default())
}

/// Whole seconds, rounded up, for estimates that shouldn't flicker every frame.
fn approx(secs: f64) -> String {
    if secs < 60.0 {
        format!("{}s", secs.ceil().max(1.0))
    } else {
        format::duration(Duration::try_from_secs_f64(secs).unwrap_or(Duration::MAX))
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

/// Stop every display mid-draw: running steps are marked failed, the last frame is drawn,
/// and nothing is drawn after it. Called by the panic hook before it prints. Returns whether
/// there were any.
pub(crate) fn abandon_all() -> bool {
    display::abandon_all()
}

impl display::Shown for Mutex<Shared> {
    /// Draw one last frame and leave it where it is, for the question to go below it.
    fn pause(&self) {
        let watch = {
            let mut s = lock(self);
            if s.paused == 0 {
                paint_frame(&mut s, false);
                // left in the scrollback: the next frame starts below the question
                s.drawn = 0;
            }
            s.paused += 1;
            s.focus = None;
            s.watch.take()
        };
        if let Some(mut watch) = watch {
            watch.stop();
        }
    }

    fn resume(&self) {
        let mut s = lock(self);
        s.paused -= 1;
        if s.paused == 0 {
            s.refocus = true;
            output::write(HIDE_CURSOR);
        }
    }

    fn log(&self, line: &str) {
        lock(self).logs.push(line.to_string());
    }

    /// Running steps are marked failed and the last frame is drawn.
    fn abandon(&self) {
        // the panicking thread might be the one holding the lock, so never wait forever
        let Some(mut s) = lock_patiently(self) else { return };
        let now = s.now();
        for step in s.steps.iter_mut().filter(|st| st.state == State::Active) {
            step.state = State::Fail;
            step.ended = Some(now);
            step.note = "interrupted by a panic".into();
        }
        paint_frame(&mut s, true);
        s.abandoned = true;
        output::write(SHOW_CURSOR);
    }
}

impl Live {
    /// A live display headed `title  subject`, e.g. `deploy  my-app → production`: what's
    /// being done, and to what.
    ///
    /// Nothing is drawn until the first [`step`](Live::step) or [`log`](Live::log), so
    /// everything chosen with the builder methods ([`fps`](Live::fps),
    /// [`remember_as`](Live::remember_as), [`spinner`](Live::spinner) and the rest) holds from
    /// the first frame. Its clock starts here.
    pub fn new(title: impl Into<String>, subject: impl Into<String>) -> Self {
        Live::with_mode(title.into(), subject.into(), Mode::now())
    }

    /// Show the header and start drawing, once.
    fn start(&self) {
        self.started.call_once(|| match self.mode {
            Mode::Plain => {
                let s = lock(&self.shared);
                output::line(&format!("{} {}", s.title, s.target));
            }
            Mode::Json => {
                let s = lock(&self.shared);
                output::emit(
                    "live",
                    &[("title", Val::Str(&s.title)), ("target", Val::Str(&s.target))],
                );
            }
            Mode::Draw => self.start_drawing(),
            Mode::Quiet | Mode::Headless => {}
        });
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
    pub fn headless(title: impl Into<String>, subject: impl Into<String>) -> Self {
        let live = Live::with_mode(title.into(), subject.into(), Mode::Headless);
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
                wants_focus: mode == Mode::Draw,
                paused: 0,
                refocus: false,
            })),
            stop: Arc::new(AtomicBool::new(false)),
            started: Once::new(),
            thread: Mutex::new(None),
            notify_after: Some(DEFAULT_NOTIFY_AFTER),
            history: Some(History::default_app()),
            mode,
            closed: false,
            prev_interrupt: Mutex::new(None),
        }
    }

    fn start_drawing(&self) {
        // the theme has to be settled before the focus watcher starts reading stdin, because
        // finding it out can mean asking the terminal and reading its reply
        crate::style::theme();
        *lock(&self.prev_interrupt) = term::guard_interrupt();
        {
            let mut s = lock(&self.shared);
            s.history = self.history.as_deref().and_then(History::open);
        }
        display::add(Arc::downgrade(&self.shared) as Weak<dyn display::Shown>);
        output::write(HIDE_CURSOR);
        let (shared, stop) = (self.shared.clone(), self.stop.clone());
        *lock(&self.thread) = Some(thread::spawn(move || {
            let mut first = true;
            while !stop.load(Ordering::Relaxed) {
                // held for a question: look again a frame on, so the answer is followed at once
                if lock(&shared).paused > 0 {
                    thread::sleep(crate::fps::frame(lock(&shared).fps));
                    continue;
                }
                draw(&shared, false);
                let frame = crate::fps::frame(lock(&shared).fps);
                if std::mem::take(&mut first) || std::mem::take(&mut lock(&shared).refocus) {
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
        // stopped here only if the display had started already
        if let Some(mut watch) = watch {
            watch.stop();
        }
        self
    }

    /// Add a pending step. Declaring every step up front shows the whole plan from the start.
    pub fn step(&self, label: impl Into<String>) -> Step {
        let step = add_step(&self.shared, None, label.into());
        self.start();
        step
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
        self.start();
        log(&self.shared, None, text.as_ref());
    }

    /// Choose how running steps animate (default: [`Spinner::Sparkle`]).
    pub fn spinner(self, spinner: Spinner) -> Self {
        lock(&self.shared).spinner = spinner;
        self
    }

    /// Choose how progress bars and estimates are drawn (default: [`BarStyle::Line`]).
    ///
    /// ```no_run
    /// use tinsel::{BarStyle, Live};
    ///
    /// let live = Live::new("render", "intro.mp4").bar(BarStyle::Waveform);
    /// ```
    pub fn bar(self, bar: BarStyle) -> Self {
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
    pub fn remember_as(mut self, name: &str) -> Self {
        self.history = Some(name.to_string());
        self
    }

    /// Don't remember step timings, or show estimates from earlier runs.
    /// [`set_history`](crate::set_history) or the `TINSEL_NO_HISTORY` environment variable
    /// does the same for every display.
    pub fn no_history(mut self) -> Self {
        self.history = None;
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
        // a display finished before anything was added still shows its header and footer
        self.start();
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
        let _ = lock(&self.thread).take().map(JoinHandle::join);
        // the last frame is drawn settled, so a step that just finished shows ● rather than mid-twinkle
        draw(&self.shared, true);
        display::remove(Arc::as_ptr(&self.shared));
        let watch = lock(&self.shared).watch.take();
        let away = watch.as_ref().is_some_and(|w| w.focus.away());
        if let Some(mut watch) = watch {
            watch.stop();
        }
        output::write(SHOW_CURSOR);
        term::restore_interrupt(lock(&self.prev_interrupt).take());

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
    /// that reports its own progress doesn't show an estimate. The fraction is kept between 0
    /// and 1, and NaN (from dividing by a total of 0, say) counts as 0.
    pub fn progress(&self, fraction: f32, text: impl Into<String>) {
        let text = text.into();
        self.with(|s| s.progress = Some((crate::widgets::fraction(fraction), text)));
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
            Mode::Quiet => output::err_line(&plain_line(step, None)),
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
            Mode::Quiet if state == State::Fail => output::err_line(&plain_line(&s.steps[self.idx], secs)),
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

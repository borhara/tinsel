//! A live region: any [`Block`], redrawn in place while something runs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::block::Block;
use crate::output::{self, Output, Val};
use crate::term;

const DEFAULT_FPS: u32 = 10;

/// How a region shows itself, decided once when it starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// Redrawn in place on a terminal.
    Draw,
    /// Logs as they come, then the last frame if it's kept.
    Plain,
    /// A `"log"` event per log line, then the last frame as a `"block"` event if it's kept.
    Json,
    /// Nothing.
    Quiet,
    /// Nothing, but a snapshot whenever asked: for tests.
    Headless,
}

impl Mode {
    fn pick(out: Output, terminal: bool) -> Mode {
        match out {
            Output::Json => Mode::Json,
            Output::Quiet => Mode::Quiet,
            Output::Pretty if terminal => Mode::Draw,
            Output::Pretty => Mode::Plain,
        }
    }

    /// Whether the last frame is printed when the region finishes, off a terminal.
    fn prints_last_frame(self, keep: bool) -> bool {
        keep && matches!(self, Mode::Plain | Mode::Json)
    }
}

type View = Box<dyn FnMut() -> Block + Send>;

struct State {
    /// Lines logged since the last frame, waiting to be printed above it. A headless region
    /// keeps them all, so snapshots show them.
    logs: Vec<String>,
    /// Rows of the last frame on screen, for the next one to draw over.
    drawn: usize,
    every: Duration,
    /// A panic took over the screen: stop drawing.
    abandoned: bool,
}

/// Any [`Block`] (a table, a tree, a few lines of status) redrawn in place at a steady rate
/// while work goes on, for displays a [`Live`](crate::Live) step list doesn't fit. The view
/// is a closure that builds the block afresh each time from whatever state it shares.
///
/// When it finishes it's cleared away, unless [`keep`](Region::keep) asks for the last frame
/// to stay. Lines [logged](Region::log) while it runs print above it and stay.
///
/// ```no_run
/// use std::sync::{Arc, Mutex};
/// use tinsel::{Region, Table};
///
/// let done = Arc::new(Mutex::new(Vec::<String>::new()));
/// let shown = done.clone();
/// let region = Region::new(move || {
///     let done = shown.lock().unwrap();
///     let mut table = Table::new(["service", "status"]);
///     for name in done.iter() {
///         table = table.row([name.as_str(), "ready"]);
///     }
///     table.lines()
/// })
/// .keep();
///
/// for name in ["api", "web", "worker"] {
///     // … start it
///     done.lock().unwrap().push(name.to_string());
///     region.log(format!("started {name}"));
/// }
/// region.finish();
/// ```
///
/// On a terminal it redraws (on stdout, or the [`Target`](crate::Target)). Anywhere else it
/// prints logged lines as they come and nothing else while it runs, then the last frame once
/// if it's kept. With [`Output::Json`](crate::Output::Json) logged lines are `"log"` events
/// and a kept last frame is one `"block"` event; with [`Output::Quiet`](crate::Output::Quiet)
/// nothing prints.
pub struct Region {
    state: Arc<Mutex<State>>,
    view: Arc<Mutex<View>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    mode: Mode,
    keep: bool,
    closed: bool,
    prev_interrupt: Option<usize>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Regions drawing right now, so a panic can stop them.
static DRAWING: Mutex<Vec<Weak<Mutex<State>>>> = Mutex::new(Vec::new());

/// Stop every region mid-draw, leaving its last frame on screen. Called by the panic hook
/// before it prints. Returns whether there were any.
pub(crate) fn abandon_all() -> bool {
    let drawing = std::mem::take(&mut *lock(&DRAWING));
    let mut any = false;
    for state in drawing.iter().filter_map(Weak::upgrade) {
        // the panicking thread might be the one holding the lock, so never wait forever
        for _ in 0..50 {
            match state.try_lock() {
                Ok(mut s) => s.abandoned = true,
                Err(TryLockError::Poisoned(e)) => e.into_inner().abandoned = true,
                Err(TryLockError::WouldBlock) => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
            }
            break;
        }
        output::write("\x1b[?7h\x1b[?25h");
        any = true;
    }
    any
}

impl Region {
    /// Start a region drawing `view`, ten times a second unless [`fps`](Region::fps) says
    /// otherwise. It starts straight away.
    pub fn new(view: impl FnMut() -> Block + Send + 'static) -> Self {
        let mode = Mode::pick(output::output(), output::is_terminal());
        let mut region = Region::with_mode(Box::new(view), mode);
        if mode == Mode::Draw {
            region.start_drawing();
        }
        region
    }

    /// A region that never draws, for checking what it shows in tests: take a
    /// [`snapshot`](Region::snapshot) whenever you like.
    ///
    /// ```
    /// use tinsel::{Block, Region};
    ///
    /// tinsel::set_color_mode(tinsel::ColorMode::None);
    /// let region = Region::headless(|| Block::from(vec!["  3 of 5 ready".to_string()]));
    /// region.log("api is up");
    /// assert_eq!(region.snapshot().plain(), "api is up\n  3 of 5 ready");
    /// ```
    pub fn headless(view: impl FnMut() -> Block + Send + 'static) -> Self {
        Region::with_mode(Box::new(view), Mode::Headless)
    }

    fn with_mode(view: View, mode: Mode) -> Self {
        Region {
            state: Arc::new(Mutex::new(State {
                logs: Vec::new(),
                drawn: 0,
                every: Duration::from_secs(1) / DEFAULT_FPS,
                abandoned: false,
            })),
            view: Arc::new(Mutex::new(view)),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            mode,
            keep: false,
            closed: false,
            prev_interrupt: None,
        }
    }

    fn start_drawing(&mut self) {
        // settled before the first frame, since finding it out can mean asking the terminal
        crate::style::theme();
        self.prev_interrupt = term::guard_interrupt();
        lock(&DRAWING).push(Arc::downgrade(&self.state));
        output::write("\x1b[?25l");
        let (state, view, stop) = (self.state.clone(), self.view.clone(), self.stop.clone());
        self.thread = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // the view runs without the region locked, so it can take its own locks freely
                let lines = (lock(&view))().into_lines();
                let mut s = lock(&state);
                let every = s.every;
                draw(&mut s, &lines, false);
                drop(s);
                thread::park_timeout(every);
            }
        }));
    }

    /// Redraw this many times a second (default 10), kept between 1 and 240. The view is
    /// called once a frame, so a slow one wants a lower rate.
    ///
    /// ```no_run
    /// let region = tinsel::Region::new(|| tinsel::pairs(&[("queue", "12 jobs")])).fps(4);
    /// ```
    pub fn fps(self, fps: u32) -> Self {
        lock(&self.state).every = crate::fps::frame(fps.clamp(1, 240));
        self
    }

    /// Leave the last frame on screen when the region finishes, rather than clearing it
    /// away. Off a terminal, the last frame is printed once at the end.
    ///
    /// ```no_run
    /// let region = tinsel::Region::new(|| tinsel::pairs(&[("status", "ready")])).keep();
    /// ```
    pub fn keep(mut self) -> Self {
        self.keep = true;
        self
    }

    /// Print a line above the region, which carries on drawing underneath it. The line is
    /// printed as given, so style it as you like (a [`Tone`](crate::Tone) line, say). Safe to
    /// call from any thread.
    ///
    /// ```no_run
    /// let region = tinsel::Region::new(|| tinsel::pairs(&[("status", "starting")]));
    /// region.log(tinsel::Tone::Success.line("api is up"));
    /// ```
    pub fn log(&self, line: impl AsRef<str>) {
        let line = line.as_ref();
        match self.mode {
            Mode::Draw | Mode::Headless => {
                lock(&self.state).logs.push(line.to_string());
                self.refresh();
            }
            Mode::Plain => output::line(line),
            Mode::Json => output::emit("log", &[("text", Val::Str(line))]),
            Mode::Quiet => {}
        }
    }

    /// Redraw now rather than at the next frame, after something changed that should show
    /// straight away.
    pub fn refresh(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    /// What the region shows right now: anything logged, then the view. Most useful with
    /// [`headless`](Region::headless) regions in tests.
    pub fn snapshot(&self) -> Block {
        let view = (lock(&self.view))();
        let mut lines = lock(&self.state).logs.clone();
        lines.extend(view.into_lines());
        lines.into()
    }

    /// Stop drawing: clear the region away, or leave its last frame if it's
    /// [kept](Region::keep). Dropping a `Region` does the same.
    pub fn finish(mut self) {
        self.close();
    }

    fn close(&mut self) {
        if std::mem::replace(&mut self.closed, true) {
            return;
        }
        if self.mode.prints_last_frame(self.keep) {
            return (lock(&self.view))().print();
        }
        if self.mode != Mode::Draw {
            return;
        }
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.take().map(|handle| {
            handle.thread().unpark();
            handle.join()
        });
        let lines = if self.keep {
            (lock(&self.view))().into_lines()
        } else {
            Vec::new()
        };
        draw(&mut lock(&self.state), &lines, true);
        lock(&DRAWING).retain(|w| w.upgrade().is_some_and(|s| !Arc::ptr_eq(&s, &self.state)));
        output::write("\x1b[?25h");
        term::restore_interrupt(self.prev_interrupt.take());
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        self.close();
    }
}

fn draw(s: &mut State, lines: &[String], last: bool) {
    if s.abandoned {
        return;
    }
    let room = term::height().saturating_sub(1).max(1);
    let frame = frame(s, lines, if last { usize::MAX } else { room });
    output::write(&frame);
}

/// The escape codes and text that draw `lines` over the last frame, with anything logged
/// since above them. Only the first `room` lines are drawn, since the cursor can't move back
/// above the top of the screen to redraw the rest.
fn frame(s: &mut State, lines: &[String], room: usize) -> String {
    let mut buf = if s.drawn > 0 {
        format!("\x1b[{}F", s.drawn)
    } else {
        String::new()
    };
    for line in s.logs.drain(..) {
        buf += &line;
        buf += "\x1b[K\n";
    }
    // no wrapping: an over-long line is cut at the edge rather than taking up two rows and
    // throwing off the next redraw
    buf += "\x1b[?7l";
    let shown = &lines[..lines.len().min(room)];
    for line in shown {
        buf += line;
        buf += "\x1b[K\n";
    }
    buf += "\x1b[J\x1b[?7h";
    s.drawn = shown.len();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State {
            logs: Vec::new(),
            drawn: 0,
            every: Duration::from_millis(100),
            abandoned: false,
        }
    }

    fn lines(ls: &[&str]) -> Vec<String> {
        ls.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn each_frame_draws_over_the_last() {
        let mut s = state();
        let first = frame(&mut s, &lines(&["a", "b"]), 20);
        assert_eq!(first, "\x1b[?7la\x1b[K\nb\x1b[K\n\x1b[J\x1b[?7h");
        let second = frame(&mut s, &lines(&["c"]), 20);
        assert!(second.starts_with("\x1b[2F"), "{second:?}");
        assert_eq!(s.drawn, 1);
    }

    #[test]
    fn logs_go_above_the_frame_and_stay() {
        let mut s = state();
        frame(&mut s, &lines(&["status"]), 20);
        s.logs.push("api is up".into());
        let next = frame(&mut s, &lines(&["status"]), 20);
        assert_eq!(next, "\x1b[1Fapi is up\x1b[K\n\x1b[?7lstatus\x1b[K\n\x1b[J\x1b[?7h");
        assert!(s.logs.is_empty());
    }

    #[test]
    fn only_what_fits_is_redrawn() {
        let mut s = state();
        let f = frame(&mut s, &lines(&["1", "2", "3", "4"]), 2);
        assert!(f.contains("2\x1b[K\n") && !f.contains("3\x1b[K\n"), "{f:?}");
        assert_eq!(s.drawn, 2);
    }

    #[test]
    fn a_cleared_region_leaves_nothing_behind() {
        let mut s = state();
        frame(&mut s, &lines(&["a", "b"]), 20);
        assert_eq!(frame(&mut s, &[], usize::MAX), "\x1b[2F\x1b[?7l\x1b[J\x1b[?7h");
        assert_eq!(s.drawn, 0);
    }

    #[test]
    fn the_last_frame_prints_off_a_terminal_only_when_kept() {
        assert_eq!(Mode::pick(Output::Pretty, true), Mode::Draw);
        assert_eq!(Mode::pick(Output::Pretty, false), Mode::Plain);
        assert!(Mode::Plain.prints_last_frame(true));
        assert!(Mode::Json.prints_last_frame(true));
        assert!(!Mode::Plain.prints_last_frame(false));
        assert!(!Mode::Json.prints_last_frame(false));
        assert!(!Mode::Quiet.prints_last_frame(true));
        assert!(!Mode::Headless.prints_last_frame(true));
        // a terminal keeps it by drawing it, not by printing it again
        assert!(!Mode::Draw.prints_last_frame(true));
    }

    use crate::output::{capture, with_output, with_terminal};

    fn view(text: &'static str) -> impl FnMut() -> Block + Send + 'static {
        move || Block::from(vec![text.to_string()])
    }

    #[test]
    fn draws_on_a_terminal_until_finished() {
        let out = capture(|| {
            with_terminal(true, || {
                let region = Region::new(view("r-view")).fps(240);
                region.log("r-log");
                std::thread::sleep(Duration::from_millis(20));
                region.finish();
                Region::new(view("r-kept")).keep().finish();
            })
        });
        assert!(out.starts_with("\x1b[?25l") && out.ends_with("\x1b[?25h"), "{out:?}");
        assert!(
            out.contains("r-view") && out.contains("r-log") && out.contains("r-kept"),
            "{out:?}"
        );
    }

    #[test]
    fn off_a_terminal_logs_are_lines_or_events() {
        let out = capture(|| {
            with_terminal(false, || {
                let region = Region::new(view("p-view"));
                region.log("p-log");
                region.refresh();
                drop(region);
                Region::new(view("p-kept")).keep().finish();
            })
        });
        assert!(
            out.contains("p-log\n") && out.contains("p-kept\n") && !out.contains("p-view"),
            "{out:?}"
        );
        let json = capture(|| {
            with_output(Output::Json, || {
                let region = Region::new(view("j-view")).keep();
                region.log("j-log");
            })
        });
        assert!(json.contains(r#"{"event":"log","text":"j-log"}"#), "{json}");
        assert!(json.contains(r#"{"event":"block","lines":["j-view"]}"#), "{json}");
        let quiet = capture(|| with_output(Output::Quiet, || Region::new(view("q-view")).keep().log("q-log")));
        assert!(!quiet.contains("q-"), "{quiet}");
    }

    #[test]
    fn a_panic_stops_every_region_mid_draw() {
        capture(|| {
            let busy = with_terminal(true, || Region::new(view("busy")));
            let poisoned = with_terminal(true, || Region::new(view("poisoned")));
            let state = poisoned.state.clone();
            let _ = std::thread::spawn(move || {
                let _held = state.lock().unwrap();
                panic!("poison the region");
            })
            .join();
            // held for a moment while abandon_all waits its turn
            let held = busy.state.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let holder = std::thread::spawn(move || {
                let _held = held.lock().unwrap();
                tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(10));
            });
            rx.recv().unwrap();
            assert!(abandon_all());
            holder.join().unwrap();
            assert!(lock(&busy.state).abandoned && lock(&poisoned.state).abandoned);
            assert!(!abandon_all());
        });
    }
}

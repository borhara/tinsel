//! Full-screen terminal apps: dashboards, pickers, little toys, in the same pink as
//! everything else.
//!
//! An app is a struct with your state, and two methods: [`view`](App::view) draws the state
//! into a [`Frame`], and [`event`](App::event) changes it when a key is pressed, the mouse
//! moves or time passes. [`run`] takes over the screen, calls them in turn, and puts the
//! terminal back when you're done (or if anything panics).
//!
//! ```no_run
//! use tinsel::app::{self, App, Event, Flow, Frame, Key, Keymap, List, Panel, Size::*, hints};
//!
//! struct Picker {
//!     flavours: Vec<&'static str>,
//!     selected: usize,
//! }
//!
//! impl App for Picker {
//!     fn view(&self, frame: &mut Frame) {
//!         let [main, footer] = frame.area().rows([Fill(1), Fixed(1)]);
//!         let list = List::new(self.flavours.clone()).selected(self.selected);
//!         frame.render(main, Panel::new("ice cream").body(list).focused(true));
//!         frame.render(footer.shrink(1, 0), hints(&[("↑↓", "choose"), ("enter", "done")]));
//!     }
//!
//!     fn event(&mut self, event: &Event) -> Flow {
//!         app::navigate(&mut self.selected, self.flavours.len(), event, Keymap::Vim);
//!         match event {
//!             Event::Key(Key::Enter | Key::Esc | Key::Char('q')) => Flow::Quit,
//!             _ => Flow::Continue,
//!         }
//!     }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     let picker = app::run(Picker { flavours: vec!["strawberry", "pistachio", "mint"], selected: 0 })?;
//!     println!("one scoop of {}", picker.flavours[picker.selected]);
//!     Ok(())
//! }
//! ```
//!
//! # Laying things out
//!
//! [`Frame::area`] is the whole screen as a [`Rect`]; [`Rect::rows`] and [`Rect::columns`]
//! cut it up by [`Size`], and [`Frame::render`] draws a [`Widget`] into a piece. Everything
//! tinsel prints ([`Table`](crate::Table), [`Tree`](crate::Tree), [`Card`](crate::Card),
//! any [`Block`]) is a widget already; on top of those there's [`Panel`], [`List`],
//! [`Grid`] (a table with a cursor), [`ScrollView`] (lines to scroll through, or follow as they
//! grow), [`TextInput`] (a box to type into) and [`TextArea`] (several lines of it), [`Tabs`], [`Plot`], [`Stat`], [`Loading`] (a spinner, and a bar once there's progress), [`big`] text, [`hints`]
//! for the footer and [`Sparkles`] for the background.
//!
//! # Time
//!
//! [`Frame::time`] is the seconds since the app started, for anything animated, and
//! [`time`] the same clock read in [`App::event`]. Both are `f64`, so they stay precise
//! however long an app runs. Whatever animates from a moment is told that moment, rather
//! than reading the clock itself: [`Modal::opened`], [`Toasts::push`] and
//! [`Toasts::showing`] all take it, so they can be drawn at any time in tests.
//!
//! # Drawing only when needed
//!
//! An app is drawn when something happens: a key, a click, a paste, a resize, a [`Waker`]
//! waking it, or the terminal gaining or losing focus. Between those it sleeps, and costs
//! nothing, unless the last frame asked to be drawn again:
//!
//! - [`Frame::animate`] while something moves, for the next frame at the app's full
//!   [`fps`](App::fps);
//! - [`Frame::animate_at`] for ambient animation (stars twinkling, a light running round a
//!   border) at a slower rate, which rests while the terminal is in the background and is
//!   timed by [`Frame::ambient_time`], a clock that rests with it;
//! - [`Frame::redraw_in`] and [`Frame::redraw_at`] for something that changes at a known
//!   moment, like a clock on the screen at the turn of the minute.
//!
//! Every frame asks afresh, so whatever stops asking stops costing. tinsel's own animated
//! widgets ([`Panel`] when focused, [`Toasts`], [`Modal::opened`], [`Loading`],
//! [`Sparkles`], a [`Spinner`](crate::Spinner) and [`Grid`] while it loads) ask for their
//! own frames. When a frame that was asked for comes due, the app is sent an [`Event::Tick`]
//! first, for polling whatever a dashboard shows. Work done on other threads can wake the
//! app the moment it's ready with a [`Waker`], so it needn't poll at all; a [`Job`] runs
//! work on a thread of its own and wakes the app with its result.
//!
//! # Typing
//!
//! The terminal's cursor is hidden while an app runs. A frame that wants it, to show where
//! typing goes, puts it somewhere with [`Frame::set_cursor`], in a [`CursorShape`]. An app
//! that shouldn't end on Ctrl-C says so with [`App::ctrl_c_quits`].
//!
//! # Leaving for a moment
//!
//! [`suspend`] hands the terminal back while something else uses it (a sign-in command, an
//! editor, a shell), then takes it again and redraws.
//!
//! # Tests
//!
//! [`snapshot`] draws one frame of an app at any size, without a terminal, so you can check
//! exactly what it shows. When stdout isn't a terminal, [`run`] prints that one frame and
//! returns.

mod field;
mod frame;
mod grid;
mod input;
mod job;
mod keys;
mod layout;
mod notation;
mod overlay;
mod schedule;
mod scroll;
mod textarea;
mod widgets;

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

pub use field::{Edit, History, TextInput};
pub use frame::{CursorShape, Frame, TerminalCursor};
pub use grid::{Grid, GridState};
pub use input::{Key, Mods, Mouse, MouseKind, NamedKey};
pub use job::Job;
pub use keys::Keys;
pub use layout::{Rect, Size};
pub use notation::ParseKeyError;
pub use overlay::{Modal, Toasts};
pub use scroll::{Scroll, ScrollView};
pub use textarea::{Position, TextArea};
pub use widgets::{Footer, List, Loading, Panel, Plot, Scrollbar, Sparkles, Stat, Tabs, Widget, big, hints};

use crate::block::Block;
use crate::term;

/// Something that happened, handed to [`App::event`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Event {
    /// A key was pressed.
    Key(Key),
    /// Text was pasted, all at once, with its line breaks as `\n`: a newline in it is part
    /// of the text, not a press of Enter. [`TextInput`] takes it as typed. Terminals that can't
    /// say what's pasted (older ones, Windows consoles outside Windows Terminal) send it as
    /// keys instead.
    Paste(String),
    /// The mouse was clicked or scrolled (only if [`App::mouse`] says so).
    Mouse(Mouse),
    /// Time passed: a frame asked for with [`Frame::animate`], [`Frame::animate_at`],
    /// [`Frame::redraw_in`] or [`Frame::redraw_at`] is due, and is drawn straight after.
    /// None arrive while nothing asks.
    Tick,
    /// The terminal changed size; also sent once as the app starts, before its first frame.
    Resize {
        /// Its width now, in columns.
        width: usize,
        /// Its height now, in rows.
        height: usize,
    },
    /// A [`Waker`] was woken: something another thread was doing is ready to look at.
    Wake,
    /// The terminal window gained (`true`) or lost (`false`) focus, where the terminal says so:
    /// a chance to pause animations nobody is watching.
    Focus(bool),
}

/// Whether to keep going after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flow {
    /// Carry on.
    Continue,
    /// Put the terminal back and return from [`run`].
    Quit,
}

/// A full-screen app. Only [`view`](App::view) is needed: by default `q` and Esc quit,
/// it's drawn when something happens or a frame asks, and the mouse is left alone.
pub trait App {
    /// Draw the app as it is now. Called for every frame, so keep it quick, and keep slow
    /// work (fetching, reading files) in [`event`](App::event) or another thread. Anything
    /// that moves asks for the next frame, as [Drawing only when
    /// needed](self#drawing-only-when-needed) says.
    fn view(&self, frame: &mut Frame);

    /// Change the app in response to something happening. Ctrl-C comes here too, so the app
    /// can note that it was stopped, and then quits, whatever this returns, unless
    /// [`ctrl_c_quits`](App::ctrl_c_quits) says otherwise.
    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(Key::Char('q') | Key::Esc) => Flow::Quit,
            _ => Flow::Continue,
        }
    }

    /// The most frames a second to draw: the rate of [`Frame::animate`], and the most any
    /// [`Frame::animate_at`] gets. [`tinsel::fps`](crate::fps) unless changed. It's asked
    /// again after every frame, so an app can change it as it goes.
    fn fps(&self) -> u32 {
        crate::fps()
    }

    /// The terminal window's title while the app runs, such as the app's name and what it
    /// shows. Left alone unless changed; the old title comes back when the app quits.
    fn title(&self) -> Option<String> {
        None
    }

    /// Whether Ctrl-C ends the app after [`event`](App::event) has seen it. Yes unless
    /// changed; an app that says no gets it as an ordinary key.
    fn ctrl_c_quits(&self) -> bool {
        true
    }

    /// Whether to report clicks, drags and the scroll wheel. Off unless changed, since while it's on
    /// the terminal can't select text with the mouse.
    fn mouse(&self) -> bool {
        false
    }
}

/// Run an app full-screen until it quits, then hand it back, so you can read what was chosen.
///
/// The screen is put back as it was afterwards, whether the app quits, returns an error or
/// panics. If the terminal goes away (its window closed), that's the error. When stdout isn't a terminal, one frame is printed instead, at the terminal's
/// size (or 80×24), after the app is told that size with an [`Event::Resize`], and the app is
/// returned straight away.
///
/// One app runs at a time: the screen and keyboard are the whole process's, so `run` called
/// while another app is running (from its [`App::event`], or another thread) is an error, and
/// the app already running carries on as it was. To show something else for a while, draw
/// it in the same app, or hand the terminal over with [`suspend`].
///
/// A panic on another thread while the app runs (in a [`Job`]'s work, say) leaves the app
/// running, and its message is kept until the app has left the screen, rather than written
/// over what the app shows.
///
/// # Errors
///
/// An I/O error if the terminal can't be set up or drawn on, or once it goes away (its window
/// closed), or if another app is already running. The terminal is put back as it was either
/// way.
pub fn run<A: App>(mut app: A) -> io::Result<A> {
    run_app(&mut app)?;
    Ok(app)
}

/// [`run`], for any app.
fn run_app(app: &mut dyn App) -> io::Result<()> {
    if !term::interactive() {
        let (width, height) = (term::width(), term::height());
        // told its size first, as it would be on a terminal; one frame is printed whatever
        // it says, since that's all there is
        app.event(&Event::Resize { width, height });
        snapshot(app, width, height).print();
        return Ok(());
    }
    // before anything is touched, so the app already running keeps the terminal as it is
    let _only = Only::claim()?;
    // decide these before stdin is in raw mode: finding out may mean asking the terminal
    crate::style::theme();
    crate::style::color_mode();

    let mut session = Session::enter(app.mouse())?;
    let (tx, rx) = mpsc::channel();
    let _waking = Waking::listen(tx.clone());
    let control = session.control.clone();
    session.reader = Some(std::thread::spawn(move || {
        let mut keyboard = term::Keyboard::default();
        let resizes = tx.clone();
        let mut read = |buf: &mut [u8]| {
            let n = keyboard.read(buf);
            // reads give up every 0.1s, so a resize is passed on that soon, however slow
            // the app ticks
            if term::resized() {
                let _ = resizes.send(Ok(Event::Resize {
                    width: term::width(),
                    height: term::height(),
                }));
            }
            n
        };
        listen(&mut read, &tx, &control);
        control.idle.store(true, Ordering::SeqCst);
    }));
    // caught so the session, and with it the panic hook, is put back while not panicking,
    // then the panic carries on to the caller
    let driven = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(app, &rx, &mut io::stdout(), &mut || (term::width(), term::height()))
    }));
    drop(session);
    match driven {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Seconds after starting that the window's colour is sent again, once nothing moves.
const SETTLE: f64 = 0.5;

/// What the key reader sends: an event, or why there'll be no more (the terminal closed).
type Typed = io::Result<Event>;

/// The loop itself: draw, wait for an event or the frame the last one asked for, and go round
/// again until the app quits, Ctrl-C is pressed or the events stop. Kept apart from the
/// terminal so it can be tested with any events, output and screen size.
fn drive(
    app: &mut dyn App,
    rx: &mpsc::Receiver<Typed>,
    out: &mut dyn Write,
    screen: &mut dyn FnMut() -> (usize, usize),
) -> io::Result<()> {
    let start = Instant::now();
    let _clock = Clock::start(start);
    // left from an app before that quit straight after it
    LOST.with(|l| l.borrow_mut().take());
    let now = || start.elapsed().as_secs_f64();
    let mut ambient = schedule::Ambient::default();
    let mut size = screen();
    let _size = Measured::start(size);
    // told before the first frame, so an app can lay itself out before it's drawn
    if app.event(&Event::Resize {
        width: size.0,
        height: size.1,
    }) == Flow::Quit
    {
        return Ok(());
    }
    let mut shown: Option<Frame> = None;
    // the frame before last, whose cells the next frame reuses
    let mut spare: Option<Frame> = None;
    let mut titled: Option<String> = None;
    let mut cursor: Option<TerminalCursor> = None;
    let mut cursor_color: Option<crate::style::Rgb> = None;
    let mut window_color: Option<crate::style::Rgb> = None;
    // whether the window's colour has been sent again once things settled after starting
    let mut settled = false;
    let mut moving = false;
    loop {
        // some terminals (iTerm among them) can keep their old margin colour through the
        // first busy moments, so it's sent again once nothing moves
        if !settled && !moving && now() >= SETTLE {
            settled = true;
            window_color = None;
        }
        if redraw_asked(true) {
            shown = None;
            titled = None;
            cursor = None;
            cursor_color = None;
            window_color = None;
        }
        let title = app.title();
        if title.is_some() && title != titled {
            // control characters would end the sequence early
            let clean: String = title
                .iter()
                .flat_map(|t| t.chars())
                .filter(|c| !c.is_control())
                .collect();
            write!(out, "\x1b]0;{clean}\x07")?;
            titled = title;
        }
        let now_size = screen();
        if now_size != size {
            size = now_size;
            SIZE.with(|s| s.set(Some(size)));
            shown = None;
            if app.event(&Event::Resize {
                width: size.0,
                height: size.1,
            }) == Flow::Quit
            {
                return Ok(());
            }
        }
        // the terminal couldn't be taken back after a suspend, so the app can't go on
        if let Some(lost) = LOST.with(|l| l.borrow_mut().take()) {
            return Err(lost);
        }
        let blank = match spare.take() {
            Some(old) => old.renew(size.0, size.1),
            None => Frame::new(size.0, size.1),
        };
        let drawn = now();
        let mut frame = blank.at(drawn).ambient_at(ambient.at(drawn));
        app.view(&mut frame);
        let diff = frame.diff(shown.as_ref());
        let (hide, place) = frame.cursor_escape(cursor, !diff.is_empty());
        let color = frame.cursor_color_escape(cursor_color) + &frame.window_color_escape(window_color);
        if !diff.is_empty() || !hide.is_empty() || !place.is_empty() || !color.is_empty() {
            write!(out, "\x1b[?2026h{color}{hide}{diff}{place}\x1b[?2026l")?;
            out.flush()?;
        }
        cursor = frame.cursor();
        cursor_color = frame.cursor_color().or(cursor_color);
        window_color = frame.window_color().or(window_color);
        let due = schedule::next_frame(frame.wants(), drawn, app.fps(), ambient.focused());
        moving = frame.wants().motion;
        spare = shown.replace(frame);

        // a redraw asked for while drawing comes straight away, but isn't time passing
        let again = redraw_asked(false);
        let settling = (!settled && !moving).then_some(SETTLE);
        let wake = [due, settling].into_iter().flatten().reduce(f64::min);
        let received = match (again, wake) {
            (true, _) => rx.try_recv().map_err(|e| e == mpsc::TryRecvError::Disconnected),
            (false, Some(wake)) => {
                let wait = Duration::from_secs_f64((wake - now()).clamp(0.0, 3600.0));
                rx.recv_timeout(wait)
                    .map_err(|e| e == mpsc::RecvTimeoutError::Disconnected)
            }
            (false, None) => rx.recv().map_err(|_| true),
        };
        let first = match received {
            Ok(event) => Some(event),
            Err(false) => None,
            // the reader stopped: stdin is gone, so nothing more can be typed
            Err(true) => return Ok(()),
        };
        for event in first.into_iter().chain(rx.try_iter()) {
            // nothing more can be typed, so the app can't be told to quit: it ends here
            let event = event?;
            // only a nudge: the size is looked at, and the app told, before the next frame
            if matches!(event, Event::Resize { .. }) {
                continue;
            }
            if event == Event::Wake {
                WOKEN.store(false, Ordering::SeqCst);
            }
            if let Event::Focus(on) = event {
                ambient.focus(on, now());
                // the window's colour too may have been lost while away
                if on {
                    window_color = None;
                }
            }
            let quit = app.event(&event) == Flow::Quit;
            if quit || (event == Event::Key(Key::Ctrl('c')) && app.ctrl_c_quits()) {
                return Ok(());
            }
        }
        // the frame asked for is due: time passed, as far as the app is concerned
        if !again && due.is_some_and(|due| now() >= due) && app.event(&Event::Tick) == Flow::Quit {
            return Ok(());
        }
    }
}

/// Where wakes go while an app is running.
static WAKE: Mutex<Option<mpsc::Sender<Typed>>> = Mutex::new(None);
/// Whether an [`Event::Wake`] is on its way and not yet handled, so a burst of wakes sends one.
static WOKEN: AtomicBool = AtomicBool::new(false);

/// Wakes the running app from another thread, so it can show what changed straight away,
/// without polling for it. Make one whenever you like (before [`run`] is fine), clone it
/// into every thread that needs it, and call [`wake`](Waker::wake) when there's something
/// new: the app gets an [`Event::Wake`] at once, and is drawn again. So an app can sleep
/// until a result arrives and show it the moment it does.
///
/// The wake says only that something happened. To hand the result over, send it down your
/// own channel and read it on `Event::Wake`:
///
/// ```no_run
/// use std::sync::mpsc;
/// use tinsel::app::{self, App, Event, Flow, Frame, Waker};
///
/// struct Status {
///     results: mpsc::Receiver<String>,
///     latest: String,
/// }
///
/// impl App for Status {
///     fn view(&self, frame: &mut Frame) {
///         frame.print(frame.area(), 0, &self.latest);
///     }
///
///     fn event(&mut self, event: &Event) -> Flow {
///         if *event == Event::Wake {
///             self.latest = self.results.try_iter().last().unwrap_or_default();
///         }
///         Flow::Continue
///     }
/// }
///
/// let (tx, rx) = mpsc::channel();
/// let waker = Waker::new();
/// std::thread::spawn(move || {
///     tx.send("all healthy".to_string()).unwrap();
///     waker.wake();
/// });
/// app::run(Status { results: rx, latest: "checking…".into() })?;
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct Waker {
    _private: (),
}

impl Waker {
    /// A waker for whichever app is running, or the next one to start.
    pub fn new() -> Self {
        Waker::default()
    }

    /// Send the app an [`Event::Wake`]. Wakes that come quicker than the app handles them
    /// arrive as one; a wake before the app starts arrives as it starts.
    pub fn wake(&self) {
        // under the lock, so an app starting meanwhile either sees this wake or is sent it
        let wake = WAKE.lock().unwrap_or_else(PoisonError::into_inner);
        if WOKEN.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(tx) = wake.as_ref() {
            let _ = tx.send(Ok(Event::Wake));
        }
    }
}

/// Wakes delivered to a running app, for as long as it lives.
struct Waking;

impl Waking {
    fn listen(tx: mpsc::Sender<Typed>) -> Waking {
        let mut wake = WAKE.lock().unwrap_or_else(PoisonError::into_inner);
        if WOKEN.load(Ordering::SeqCst) {
            let _ = tx.send(Ok(Event::Wake));
        }
        *wake = Some(tx);
        Waking
    }
}

impl Drop for Waking {
    fn drop(&mut self) {
        *WAKE.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

thread_local! {
    static STARTED: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static SIZE: std::cell::Cell<Option<(usize, usize)>> = const { std::cell::Cell::new(None) };
}

/// The running app's screen size, for [`size`]; forgotten however the loop ends.
struct Measured;

impl Measured {
    fn start(size: (usize, usize)) -> Measured {
        SIZE.with(|s| s.set(Some(size)));
        Measured
    }
}

impl Drop for Measured {
    fn drop(&mut self) {
        SIZE.with(|s| s.set(None));
    }
}

/// The running app's screen, as `(width, height)` in columns and rows: the size its frames
/// are drawn at, for working out a page's height in [`App::event`]. Outside a running app,
/// the terminal's size (80×24 when there's no terminal).
///
/// ```
/// let (width, height) = tinsel::app::size();
/// assert!(width > 0 && height > 0);
/// ```
pub fn size() -> (usize, usize) {
    SIZE.with(std::cell::Cell::get)
        .unwrap_or_else(|| (term::width(), term::height()))
}

/// The running app's clock, for [`time`]; stopped however the loop ends.
struct Clock;

impl Clock {
    fn start(at: Instant) -> Clock {
        STARTED.with(|s| s.set(Some(at)));
        Clock
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        STARTED.with(|s| s.set(None));
    }
}

/// Send the whole screen again with the next frame, colours and cursor included, as after
/// [`suspend`]: for a terminal that may have lost track of what it shows. Works from any
/// thread: called from another, it wakes the app to draw. Does nothing when no app is running.
///
/// ```
/// tinsel::app::redraw();
/// ```
pub fn redraw() {
    // on the app's own thread, while it runs
    if STARTED.with(std::cell::Cell::get).is_some() {
        REDRAW.with(|r| r.set(true));
        return;
    }
    let wake = WAKE.lock().unwrap_or_else(PoisonError::into_inner);
    // nobody to tell, and nothing for a later app to redraw
    let Some(tx) = wake.as_ref() else { return };
    REDRAW_ELSEWHERE.store(true, Ordering::SeqCst);
    // a resize is only a nudge to the loop, which looks at the size itself, so this wakes
    // it to draw without telling the app anything
    let _ = tx.send(Ok(Event::Resize { width: 0, height: 0 }));
}

/// Whether the screen is to be sent again from scratch, asked on this thread or, if this is
/// the running app's, on another. `take` lowers it again.
fn redraw_asked(take: bool) -> bool {
    let here = REDRAW.with(|r| if take { r.replace(false) } else { r.get() });
    let ours = RUNNING.with(|r| r.borrow().is_some());
    let elsewhere = ours
        && if take {
            REDRAW_ELSEWHERE.swap(false, Ordering::SeqCst)
        } else {
            REDRAW_ELSEWHERE.load(Ordering::SeqCst)
        };
    here || elsewhere
}

/// Seconds since the app started, on the same clock as [`Frame::time`], so something that
/// happens in [`App::event`] can be animated from that moment in [`App::view`] (a
/// [`Modal`] rising into place). 0 when no app is running, as in tests.
///
/// ```
/// assert_eq!(tinsel::app::time(), 0.0);
/// ```
pub fn time() -> f64 {
    STARTED.with(|s| s.get()).map_or(0.0, |at| at.elapsed().as_secs_f64())
}

/// One frame of an app at `width` by `height`, as text: for tests, and for printing what a
/// dashboard shows into a log.
///
/// ```
/// use tinsel::app::{App, Frame, Panel};
///
/// struct Hello;
///
/// impl App for Hello {
///     fn view(&self, frame: &mut Frame) {
///         frame.render(frame.area(), Panel::new("hello").body("hi there"));
///     }
/// }
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let shown = tinsel::app::snapshot(&Hello, 16, 3).plain();
/// assert_eq!(shown.lines().nth(1), Some("│ hi there     │"));
/// ```
pub fn snapshot<A: App + ?Sized>(app: &A, width: usize, height: usize) -> Block {
    let mut frame = Frame::new(width, height);
    app.view(&mut frame);
    frame.snapshot()
}

/// Which keys move through a list or scroll a view, for [`navigate`], [`Scroll`] and
/// [`GridState`]. Both have ↑↓ a step at a time, Page Up/Down a page, Home/End to the ends
/// and the scroll wheel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Keymap {
    /// The arrows and the keys above them, and nothing else, so letters are left free for
    /// typing or the app's own shortcuts.
    Arrows,
    /// Those, and vim's j/k a step at a time and g/G to the ends. The default for a
    /// [`Scroll`] and a [`GridState`].
    #[default]
    Vim,
    /// For a list under a box being typed into, as in a file picker: the arrows and the
    /// keys above them, and Ctrl-N, Ctrl-J and Tab a step down, Ctrl-P, Ctrl-K and
    /// Shift-Tab a step up, leaving every letter for the box.
    Picker,
}

/// Move a selection through a list of `len` things in response to an event, with the keys
/// `keymap` names: ↑↓ one at a time, Page Up/Down ten, Home/End to the ends and the scroll
/// wheel, and j/k and g/G too with [`Keymap::Vim`]. Returns whether it moved. Made for
/// [`List`], but works for anything with a position.
///
/// ```
/// use tinsel::app::{Event, Key, Keymap, navigate};
///
/// let mut selected = 0;
/// navigate(&mut selected, 3, &Event::Key(Key::Down), Keymap::Arrows);
/// navigate(&mut selected, 3, &Event::Key(Key::Char('G')), Keymap::Vim);
/// assert_eq!(selected, 2);
/// assert!(!navigate(&mut selected, 3, &Event::Key(Key::Char('k')), Keymap::Arrows));
/// ```
pub fn navigate(selected: &mut usize, len: usize, event: &Event, keymap: Keymap) -> bool {
    let Some((to, _)) = step(*selected, len.saturating_sub(1), 10, 1, event, keymap) else {
        return false;
    };
    let moved = to != *selected;
    *selected = to;
    moved
}

/// The first row to show of a list scrolled to `top`, `rows` tall, so that `selected` is
/// in view, scrolling as little as it can: for a list whose selection moves by more than
/// the keys [`navigate`] knows, or that changes under it as a filter is typed.
///
/// ```
/// use tinsel::app::keep_in_view;
///
/// assert_eq!(keep_in_view(0, 3, 10), 0, "already showing");
/// assert_eq!(keep_in_view(0, 12, 10), 3, "just far enough to show it at the bottom");
/// assert_eq!(keep_in_view(8, 2, 10), 2, "or at the top");
/// ```
pub fn keep_in_view(top: usize, selected: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    if selected < top {
        selected
    } else if selected >= top + rows {
        selected + 1 - rows
    } else {
        top
    }
}

/// Where the keys `keymap` names, and the wheel, take position `at` in `0..=last`: a page at
/// a time `page` apart, a turn of the wheel `wheel`. Gives the new position, and whether the
/// way was back towards the start, or nothing for any other event.
fn step(at: usize, last: usize, page: usize, wheel: usize, event: &Event, keymap: Keymap) -> Option<(usize, bool)> {
    let vim = keymap == Keymap::Vim;
    let picker = keymap == Keymap::Picker;
    let (to, back) = match event {
        Event::Key(Key::Up) => (at.saturating_sub(1), true),
        Event::Key(Key::Down) => (at + 1, false),
        Event::Key(Key::Ctrl('p' | 'k') | Key::BackTab) if picker => (at.saturating_sub(1), true),
        Event::Key(Key::Ctrl('n' | 'j') | Key::Tab) if picker => (at + 1, false),
        Event::Key(Key::Char('k')) if vim => (at.saturating_sub(1), true),
        Event::Key(Key::Char('j')) if vim => (at + 1, false),
        Event::Key(Key::PageUp) => (at.saturating_sub(page), true),
        Event::Key(Key::PageDown) => (at + page, false),
        Event::Key(Key::Home) => (0, true),
        Event::Key(Key::End) => (last, false),
        Event::Key(Key::Char('g')) if vim => (0, true),
        Event::Key(Key::Char('G')) if vim => (last, false),
        Event::Mouse(m) if m.kind == MouseKind::ScrollUp => (at.saturating_sub(wheel), true),
        Event::Mouse(m) if m.kind == MouseKind::ScrollDown => (at + wheel, false),
        _ => return None,
    };
    Some((to.min(last), back))
}

// MARK: - The terminal

/// Whether an app has the screen, so a panic can give it back before its message is printed.
static ON_SCREEN: AtomicBool = AtomicBool::new(false);
/// Whether an app is running, anywhere in the process.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Set by [`redraw`] on a thread other than the app's.
static REDRAW_ELSEWHERE: AtomicBool = AtomicBool::new(false);
/// Panic messages from other threads while an app had the screen, to print once it's left.
static KEPT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The one app allowed to run at a time, for as long as it lives.
struct Only;

impl Only {
    fn claim() -> io::Result<Only> {
        if ACTIVE.swap(true, Ordering::SeqCst) {
            return Err(io::Error::other("an app is already running"));
        }
        Ok(Only)
    }
}

impl Drop for Only {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

// the window title is saved on entering (22t) and put back on leaving (23t)
// and pastes arrive bracketed (2004), so they can be told apart from typing,
// and the terminal reports focus coming and going (1004); with the mouse on, presses,
// releases and drags (1000, 1002) come in SGR form (1006)
const ENTER: &str = "\x1b[22;0t\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[?2004h\x1b[2J\x1b[?1004h";
const LEAVE: &str = "\x1b[0m\x1b]112\x07\x1b]111\x07\x1b[0 q\x1b[?1002l\x1b[?1000l\x1b[?1006l\x1b[?2004l\x1b[?7h\x1b[?25h\x1b[?1049l\x1b[23;0t\x1b[?1004l";
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";

/// The alternate screen, raw keys and a hidden cursor, for as long as it lives.
struct Session {
    control: Arc<Control>,
    reader: Option<std::thread::JoinHandle<()>>,
    /// The resize handler there was before, to put back.
    resize: Option<usize>,
    /// The same for hangups and kills.
    termination: [Option<usize>; 2],
    _hook: Hook,
}

/// How the app's thread tells the key reader what to do.
#[derive(Default)]
struct Control {
    /// Finish for good.
    stop: AtomicBool,
    /// Stop reading for now: something else wants the keyboard.
    pause: AtomicBool,
    /// Set by the reader while it isn't reading, and won't until `pause` is lifted.
    idle: AtomicBool,
}

/// What [`suspend`] needs of the running app's session. Raw mode lives here rather than in
/// the [`Session`], so the terminal can be handed back and taken again in the middle of a run.
struct Running {
    raw: Option<term::RawMode>,
    control: Arc<Control>,
    mouse: bool,
}

thread_local! {
    static RUNNING: std::cell::RefCell<Option<Running>> = const { std::cell::RefCell::new(None) };
    /// Set when the screen has to be drawn again from scratch.
    static REDRAW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Why the terminal couldn't be taken back after a suspend, for the loop to end with.
    static LOST: std::cell::RefCell<Option<io::Error>> = const { std::cell::RefCell::new(None) };
}

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

impl Session {
    fn enter(mouse: bool) -> io::Result<Session> {
        term::enable_escapes();
        term::remember_terminal();
        // let go of the saved settings if raw mode can't be had, or the count never gets back to zero
        let raw = term::RawMode::enter().inspect_err(|_| term::forget_terminal())?;
        let owner = std::thread::current().id();
        let hook = Hook::install(move |info| on_panic(info, owner));
        ON_SCREEN.store(true, Ordering::SeqCst);
        // made before writing anything, so a failed write still puts everything back
        let session = Session {
            control: Arc::default(),
            reader: None,
            resize: term::watch_resize(),
            termination: term::guard_termination(LEAVE),
            _hook: hook,
        };
        RUNNING.with(|r| {
            *r.borrow_mut() = Some(Running {
                raw: Some(raw),
                control: session.control.clone(),
                mouse,
            })
        });
        enter_screen(mouse)?;
        Ok(session)
    }
}

fn enter_screen(mouse: bool) -> io::Result<()> {
    let mut out = io::stdout();
    write!(out, "{ENTER}{}", if mouse { MOUSE_ON } else { "" })?;
    out.flush()
}

impl Drop for Session {
    fn drop(&mut self) {
        // the reader has to finish while reads still time out, on every way out of `run`:
        // once the terminal is back in line mode, a read waits for Enter and would take the
        // first key of whatever reads stdin next
        self.control.stop.store(true, Ordering::SeqCst);
        let _ = self.reader.take().map(std::thread::JoinHandle::join);
        term::unwatch_resize(self.resize.take());
        leave_screen();
        // out of line mode, unless a suspended closure panicked and it's already there
        RUNNING.with(|r| r.borrow_mut().take());
        print_kept();
        term::unguard_termination(std::mem::take(&mut self.termination));
        term::forget_terminal();
    }
}

/// A panic hook that does something first and then, if that says to, hands on to the one it
/// replaced, which is put back when this is dropped.
struct Hook {
    /// Lowered once dropped, for when the hook can't be taken out again.
    active: Arc<AtomicBool>,
    previous: Option<Arc<PanicHook>>,
    /// Where the hook lives, to tell it from one set since.
    ours: usize,
}

impl Hook {
    fn install(first: impl Fn(&std::panic::PanicHookInfo<'_>) -> bool + Sync + Send + 'static) -> Hook {
        let previous: Arc<PanicHook> = Arc::new(std::panic::take_hook());
        let active = Arc::new(AtomicBool::new(true));
        let (on, next) = (active.clone(), previous.clone());
        let hook: PanicHook = Box::new(move |info| {
            if !on.load(Ordering::SeqCst) || first(info) {
                next(info);
            }
        });
        let ours = hook_address(&hook);
        std::panic::set_hook(hook);
        Hook {
            active,
            previous: Some(previous),
            ours,
        }
    }
}

fn hook_address(hook: &PanicHook) -> usize {
    &**hook as *const (dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send) as *const () as usize
}

impl Drop for Hook {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        // hooks can't be changed while panicking; `run` drops its session after catching a
        // panic, so this only happens if the session is dropped some other way, and then the
        // hook stays, doing nothing but hand on
        if std::thread::panicking() {
            return;
        }
        let current = std::panic::take_hook();
        if hook_address(&current) != self.ours {
            // someone set their own since, which stays
            std::panic::set_hook(current);
            return;
        }
        drop(current);
        let Some(previous) = self.previous.take() else { return };
        match Arc::try_unwrap(previous) {
            Ok(previous) => std::panic::set_hook(previous),
            Err(previous) => std::panic::set_hook(Box::new(move |info| previous(info))),
        }
    }
}

/// What the app's panic hook does first, on the app's thread `owner` or any other. Gives
/// whether to hand on to the hook before, which prints the message.
fn on_panic(info: &std::panic::PanicHookInfo<'_>, owner: std::thread::ThreadId) -> bool {
    // give the screen back before the message, which would otherwise be drawn on the
    // alternate screen and vanish with it. Only a panic on the app's thread ends the app (or
    // any panic, with panic = "abort")
    if std::thread::current().id() == owner || cfg!(panic = "abort") {
        leave_screen();
        // unwinding drops the session, which puts the terminal back once the reader has
        // stopped; with panic = "abort" nothing else will
        if cfg!(panic = "abort") {
            term::restore_saved_terminal();
            print_kept();
        }
        return true;
    }
    // a worker thread panicking leaves the app running, so its message would be written
    // over the app, and stay there: it's kept for later instead
    !(ON_SCREEN.load(Ordering::SeqCst) && keep(info))
}

/// Keep a panic's message, as Rust would print it, for [`print_kept`]. Gives whether it was kept.
fn keep(info: &std::panic::PanicHookInfo<'_>) -> bool {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Box<dyn Any>".into());
    let thread = std::thread::current();
    let at = info.location().map(|at| format!(" at {at}")).unwrap_or_default();
    let mut text = format!(
        "thread '{}' panicked{at}:\n{message}\n",
        thread.name().unwrap_or("<unnamed>")
    );
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
        text += &format!("stack backtrace:\n{backtrace}\n");
    }
    // nothing panics while this is held, so it's never held by the thread panicking now
    KEPT.lock().unwrap_or_else(PoisonError::into_inner).push(text);
    true
}

/// Print the panic messages kept while an app had the screen, once it's left it.
fn print_kept() {
    let kept = std::mem::take(&mut *KEPT.lock().unwrap_or_else(PoisonError::into_inner));
    if kept.is_empty() {
        return;
    }
    let mut err = io::stderr();
    for text in kept {
        let _ = err.write_all(text.as_bytes());
    }
    let _ = err.flush();
}

fn leave_screen() {
    if ON_SCREEN.swap(false, Ordering::SeqCst) {
        let mut out = io::stdout();
        let _ = out.write_all(LEAVE.as_bytes());
        let _ = out.flush();
    }
}

/// Leave the full-screen app for a moment, to run something that needs the terminal as it
/// normally is: a command that asks for a password, opens an editor, or prints a sign-in
/// link and waits. The screen and keyboard are handed back, `f` runs, and then the app takes
/// them again and redraws from scratch. Call it from [`App::event`].
///
/// While `f` runs, Ctrl-C goes to the commands it starts, not the app. If `f` panics, the
/// terminal is left as it should be. If the app can't take the terminal back afterwards,
/// that's the error, and the app should quit. Outside a running app (in tests, say) `f` simply runs.
///
/// ```no_run
/// use std::process::Command;
/// use tinsel::app::{self, App, Event, Flow, Frame, Key};
///
/// struct Account {
///     signed_in: bool,
/// }
///
/// impl App for Account {
///     fn view(&self, frame: &mut Frame) {
///         let state = if self.signed_in { "signed in" } else { "s to sign in" };
///         frame.print(frame.area(), 0, state);
///     }
///
///     fn event(&mut self, event: &Event) -> Flow {
///         match event {
///             Event::Key(Key::Char('s')) => {
///                 let login = app::suspend(|| Command::new("my-cli").arg("login").status());
///                 self.signed_in = matches!(login, Ok(Ok(status)) if status.success());
///             }
///             Event::Key(Key::Char('q')) => return Flow::Quit,
///             _ => {}
///         }
///         Flow::Continue
///     }
/// }
/// ```
///
/// # Errors
///
/// An I/O error if the app can't take the terminal back afterwards.
pub fn suspend<T>(f: impl FnOnce() -> T) -> io::Result<T> {
    let mut f = Some(f);
    let mut out = None;
    suspended(&mut || out = f.take().map(|f| f()))?;
    // `suspended` has run it by the time it returns Ok
    out.ok_or(io::ErrorKind::Other.into())
}

/// [`suspend`], for any closure.
fn suspended(f: &mut dyn FnMut()) -> io::Result<()> {
    let Some((control, mouse)) = RUNNING.with(|r| r.borrow().as_ref().map(|r| (r.control.clone(), r.mouse))) else {
        f();
        return Ok(());
    };
    // the reader has to be out of its read before line mode is back, or it takes the first
    // thing typed; reads give up every 0.1s, so this is quick
    control.pause.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !control.idle.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    leave_screen();
    RUNNING.with(|r| r.borrow_mut().as_mut().map(|r| r.raw.take()));
    print_kept();

    {
        let _ctrl_c = interrupt::Pass::new();
        f();
    }

    let back = term::RawMode::enter().and_then(|raw| {
        RUNNING.with(|r| r.borrow_mut().as_mut().map(|r| r.raw = Some(raw)));
        ON_SCREEN.store(true, Ordering::SeqCst);
        enter_screen(mouse)
    });
    if let Err(e) = back {
        // the reader stays paused, and the loop ends with this before drawing again, so the
        // app doesn't run on half put back whatever it does with the error
        lose(io::Error::new(e.kind(), e.to_string()));
        return Err(e);
    }
    REDRAW.with(|r| r.set(true));
    control.pause.store(false, Ordering::SeqCst);
    Ok(())
}

/// End the running app's loop with `error`, before its next frame.
fn lose(error: io::Error) {
    LOST.with(|l| *l.borrow_mut() = Some(error));
}

/// Ctrl-C while suspended: the app survives it, and whatever it's running gets it.
#[cfg(unix)]
mod interrupt {
    unsafe extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }

    const SIGINT: i32 = 2;
    const SIG_ERR: usize = usize::MAX;

    // a handler that does nothing, rather than ignoring the signal: ignoring is inherited by
    // the commands run, so they couldn't be stopped either, while a handler is reset for them
    extern "C" fn pass(_: i32) {}

    pub(super) struct Pass(usize);

    impl Pass {
        pub(super) fn new() -> Pass {
            // SAFETY: `pass` does nothing at all, which is async-signal-safe
            Pass(unsafe { signal(SIGINT, pass as extern "C" fn(i32) as usize) })
        }
    }

    impl Drop for Pass {
        fn drop(&mut self) {
            // SAFETY: the previous handler, as signal() returned it
            (self.0 != SIG_ERR).then(|| unsafe { signal(SIGINT, self.0) });
        }
    }
}

#[cfg(not(unix))]
mod interrupt {
    pub(super) struct Pass;

    impl Pass {
        pub(super) fn new() -> Pass {
            Pass
        }
    }
}

/// Read keys and mouse reports until told to stop, sending each as an event. `read` fills
/// some of the buffer, or gives 0 after a short pause with nothing typed. If reading fails,
/// that's sent too, so the app ends rather than running on with no keyboard.
fn listen(read: &mut dyn FnMut(&mut [u8]) -> io::Result<usize>, tx: &mpsc::Sender<Typed>, control: &Control) {
    let mut decoder = input::Decoder::new();
    let mut buf = [0u8; 4096];
    while !control.stop.load(Ordering::SeqCst) {
        // busy first, then a look for a pause: a pause asked for before the look is seen,
        // and one asked for after it waits for this read, which `suspend` sees as busy
        control.idle.store(false, Ordering::SeqCst);
        if control.pause.load(Ordering::SeqCst) {
            control.idle.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        // reads give up after 0.1s, so `stop` is checked often and a pause can end a sequence
        let n = match read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        decoder.push(&buf[..n]);
        while let Some(event) = decoder.next(n == 0) {
            if let Some(event) = event {
                if tx.send(Ok(event)).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Remembers every event, and quits when told to.
    #[derive(Default)]
    struct Recorder {
        seen: Vec<Event>,
        quit_on: Option<Event>,
        quit_after_ticks: Option<usize>,
        slow: bool,
    }

    impl App for Recorder {
        fn view(&self, frame: &mut Frame) {
            frame.print(frame.area(), 0, &format!("seen {}", self.seen.len()));
            frame.animate();
        }

        fn event(&mut self, event: &Event) -> Flow {
            self.seen.push(event.clone());
            let ticks = self.seen.iter().filter(|e| **e == Event::Tick).count();
            if Some(event) == self.quit_on.as_ref() || self.quit_after_ticks == Some(ticks) {
                return Flow::Quit;
            }
            Flow::Continue
        }

        fn fps(&self) -> u32 {
            if self.slow { 1 } else { 240 }
        }
    }

    /// A reader that gives these reads in turn, as the terminal would.
    fn reads(script: Vec<io::Result<&'static [u8]>>) -> impl FnMut(&mut [u8]) -> io::Result<usize> {
        let mut script = script.into_iter();
        move |buf| {
            let bytes = script.next().unwrap_or(Ok(b""))?;
            buf[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    fn keys(rx_events: &[Event]) -> mpsc::Receiver<Typed> {
        let (tx, rx) = mpsc::channel();
        for e in rx_events {
            tx.send(Ok(e.clone())).unwrap();
        }
        // kept open, so the loop waits on ticks rather than seeing the events stop
        std::mem::forget(tx);
        rx
    }

    #[test]
    fn the_loop_hands_events_over_and_stops_on_ctrl_c() {
        let mut app = Recorder::default();
        let rx = keys(&[
            Event::Key(Key::Char('x')),
            Event::Key(Key::Ctrl('c')),
            Event::Key(Key::Char('y')),
        ]);
        let mut out = Vec::new();
        drive(&mut app, &rx, &mut out, &mut || (20, 2)).unwrap();
        // the app sees Ctrl-C, which quits even so, and nothing after it is handled
        assert_eq!(
            app.seen,
            [
                Event::Resize { width: 20, height: 2 },
                Event::Key(Key::Char('x')),
                Event::Key(Key::Ctrl('c'))
            ]
        );
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("seen 1") && out.starts_with("\x1b[?2026h"), "{out:?}");
    }

    #[test]
    fn the_title_is_set_once_until_it_changes() {
        struct Titled(usize);
        impl App for Titled {
            fn view(&self, frame: &mut Frame) {
                frame.animate();
            }
            fn event(&mut self, event: &Event) -> Flow {
                self.0 += usize::from(*event == Event::Tick);
                if self.0 == 4 { Flow::Quit } else { Flow::Continue }
            }
            fn title(&self) -> Option<String> {
                Some(if self.0 < 2 { "one\x07" } else { "two" }.into())
            }
        }
        let rx = keys(&[]);
        let mut out = Vec::new();
        drive(&mut Titled(0), &rx, &mut out, &mut || (10, 2)).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out.matches("\x1b]0;").count(), 2, "{out:?}");
        assert!(
            out.contains("\x1b]0;one\x07") && out.contains("\x1b]0;two\x07"),
            "{out:?}"
        );
    }

    #[test]
    fn the_loop_ticks_resizes_and_quits_when_the_app_says() {
        let mut app = Recorder {
            quit_after_ticks: Some(2),
            ..Recorder::default()
        };
        let rx = keys(&[]);
        let sizes = std::cell::Cell::new(0);
        let mut screen = || {
            sizes.set(sizes.get() + 1);
            if sizes.get() > 2 { (30, 4) } else { (20, 2) }
        };
        drive(&mut app, &rx, &mut Vec::new(), &mut screen).unwrap();
        assert_eq!(app.seen.iter().filter(|e| **e == Event::Tick).count(), 2);
        assert!(
            app.seen.contains(&Event::Resize { width: 30, height: 4 }),
            "{:?}",
            app.seen
        );

        // and a resize the app quits on ends it there
        let mut app = Recorder {
            quit_on: Some(Event::Resize { width: 9, height: 9 }),
            ..Recorder::default()
        };
        let mut first = true;
        let mut screen = || if std::mem::take(&mut first) { (5, 5) } else { (9, 9) };
        drive(&mut app, &rx, &mut Vec::new(), &mut screen).unwrap();
        assert_eq!(
            app.seen,
            [
                Event::Resize { width: 5, height: 5 },
                Event::Resize { width: 9, height: 9 }
            ]
        );

        // a resize the reader passes on is seen at once, however slowly the app ticks, and
        // only once, at the size the screen says
        let mut app = Recorder {
            quit_on: Some(Event::Resize { width: 9, height: 9 }),
            slow: true,
            ..Recorder::default()
        };
        let rx = keys(&[Event::Resize { width: 1, height: 1 }]);
        let mut first = true;
        let mut screen = || if std::mem::take(&mut first) { (5, 5) } else { (9, 9) };
        let started = Instant::now();
        drive(&mut app, &rx, &mut Vec::new(), &mut screen).unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(
            app.seen,
            [
                Event::Resize { width: 5, height: 5 },
                Event::Resize { width: 9, height: 9 }
            ]
        );
    }

    #[test]
    fn an_alt_key_is_not_esc_so_it_doesnt_quit() {
        struct Idle;
        impl App for Idle {
            fn view(&self, _: &mut Frame) {}
        }
        assert_eq!(Idle.event(&Event::Key(Key::Alt('x'))), Flow::Continue);
        assert_eq!(
            Idle.event(&Event::Key(Key::Mod(Mods::ALT, NamedKey::Up))),
            Flow::Continue
        );
        assert_eq!(Idle.event(&Event::Key(Key::Esc)), Flow::Quit);
    }

    #[test]
    fn ctrl_c_is_a_key_like_any_other_when_the_app_says_so() {
        struct Keeps(Vec<Event>);
        impl App for Keeps {
            fn view(&self, _: &mut Frame) {}
            fn event(&mut self, event: &Event) -> Flow {
                self.0.push(event.clone());
                if *event == Event::Key(Key::Char('q')) {
                    Flow::Quit
                } else {
                    Flow::Continue
                }
            }
            fn ctrl_c_quits(&self) -> bool {
                false
            }
        }
        let rx = keys(&[Event::Key(Key::Ctrl('c')), Event::Key(Key::Char('q'))]);
        let mut app = Keeps(Vec::new());
        drive(&mut app, &rx, &mut Vec::new(), &mut || (10, 1)).unwrap();
        assert_eq!(
            app.0,
            [
                Event::Resize { width: 10, height: 1 },
                Event::Key(Key::Ctrl('c')),
                Event::Key(Key::Char('q'))
            ]
        );
    }

    #[test]
    fn the_cursor_shows_where_the_frame_puts_it_and_hides_when_it_doesnt() {
        struct Typing(usize);
        impl App for Typing {
            fn view(&self, frame: &mut Frame) {
                frame.print(frame.area(), 0, &"x".repeat(self.0));
                if self.0 < 3 {
                    frame.set_cursor(
                        self.0,
                        0,
                        if self.0 == 0 {
                            CursorShape::Block
                        } else {
                            CursorShape::Bar
                        },
                    );
                }
                frame.animate();
            }
            fn event(&mut self, event: &Event) -> Flow {
                self.0 += usize::from(*event == Event::Tick);
                if self.0 == 4 { Flow::Quit } else { Flow::Continue }
            }
            fn fps(&self) -> u32 {
                240
            }
        }
        let rx = keys(&[]);
        let mut out = Vec::new();
        drive(&mut Typing(0), &rx, &mut out, &mut || (10, 1)).unwrap();
        let out = String::from_utf8(out).unwrap();
        // shown in its shape, moved without the shape again, then hidden, each frame sending
        // only the letter it added
        assert!(out.contains("\x1b[1;1H\x1b[2 q\x1b[?25h"), "{out:?}");
        assert!(out.contains("\x1b[1;2H\x1b[6 q\x1b[?25h"), "{out:?}");
        assert!(out.contains("\x1b[1;3H\x1b[?25h"), "{out:?}");
        assert!(out.ends_with("\x1b[?25l\x1b[1;3H\x1b[0mx\x1b[?2026l"), "{out:?}");
    }

    #[test]
    fn the_frame_rate_can_change_while_running() {
        struct Speeds(u32);
        impl App for Speeds {
            fn view(&self, frame: &mut Frame) {
                frame.animate();
            }
            fn event(&mut self, event: &Event) -> Flow {
                self.0 += u32::from(*event == Event::Tick);
                if self.0 == 20 { Flow::Quit } else { Flow::Continue }
            }
            fn fps(&self) -> u32 {
                if self.0 == 0 { 1 } else { 240 }
            }
        }
        let rx = keys(&[]);
        let started = Instant::now();
        drive(&mut Speeds(0), &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        let took = started.elapsed();
        // one slow tick, then nineteen fast ones
        assert!(took >= Duration::from_millis(900), "{took:?}");
        assert!(took < Duration::from_millis(1900), "{took:?}");
    }

    #[test]
    fn a_redraw_sends_everything_again() {
        struct Asks(u32);
        impl App for Asks {
            fn view(&self, frame: &mut Frame) {
                frame.print(frame.area(), 0, "same");
                frame.set_window_color(crate::Rgb(1, 2, 3));
                frame.animate();
            }
            fn event(&mut self, event: &Event) -> Flow {
                self.0 += u32::from(*event == Event::Tick);
                if self.0 == 1 {
                    redraw();
                }
                if self.0 == 3 { Flow::Quit } else { Flow::Continue }
            }
            fn fps(&self) -> u32 {
                240
            }
        }
        crate::with_color_mode(crate::ColorMode::TrueColor, || {
            let rx = keys(&[]);
            let mut out = Vec::new();
            drive(&mut Asks(0), &rx, &mut out, &mut || (4, 1)).unwrap();
            let out = String::from_utf8(out).unwrap();
            assert_eq!(out.matches("same").count(), 2, "{out:?}");
            assert_eq!(out.matches("\x1b]11;").count(), 2, "{out:?}");
        });
    }

    #[test]
    fn the_loop_ends_when_the_events_do() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let mut app = Recorder::default();
        drive(&mut app, &rx, &mut Vec::new(), &mut || (10, 1)).unwrap();
        // told its size before the first frame, and nothing after
        assert_eq!(app.seen, [Event::Resize { width: 10, height: 1 }]);

        // or when the reader says the terminal has gone, even with wakes still possible
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(Event::Key(Key::Char('x')))).unwrap();
        tx.send(Err(io::ErrorKind::UnexpectedEof.into())).unwrap();
        let mut app = Recorder::default();
        let err = drive(&mut app, &rx, &mut Vec::new(), &mut || (10, 1)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(
            app.seen,
            [Event::Resize { width: 10, height: 1 }, Event::Key(Key::Char('x'))]
        );
        drop(tx);
    }

    #[test]
    fn the_loop_reports_output_that_fails() {
        /// Takes everything, then can't pass it on.
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("gone"))
            }
        }
        let rx = keys(&[]);
        assert!(drive(&mut Recorder::default(), &rx, &mut Broken, &mut || (10, 1)).is_err());
    }

    fn events(rx: &mpsc::Receiver<Typed>) -> Vec<Event> {
        rx.try_iter().map(Result::unwrap).collect()
    }

    /// Held by tests that wake, since wakes go to whichever app is listening.
    pub(crate) static WAKES: Mutex<()> = Mutex::new(());

    #[test]
    fn wakes_reach_the_app_once_per_burst() {
        let _wakes = WAKES.lock().unwrap_or_else(PoisonError::into_inner);
        let (tx, rx) = mpsc::channel();
        let waker = Waker::new();
        WOKEN.store(false, Ordering::SeqCst);
        // woken before the app starts: delivered as it starts
        waker.wake();
        let waking = Waking::listen(tx);
        let other = waker.clone();
        std::thread::spawn(move || {
            other.wake();
            other.wake();
        })
        .join()
        .unwrap();
        assert_eq!(events(&rx), [Event::Wake]);

        // once handled, the next wake is sent again
        let mut app = Recorder {
            quit_on: Some(Event::Wake),
            ..Recorder::default()
        };
        let (tx2, rx2) = mpsc::channel();
        tx2.send(Ok(Event::Wake)).unwrap();
        drive(&mut app, &rx2, &mut Vec::new(), &mut || (10, 1)).unwrap();
        waker.wake();
        assert_eq!(events(&rx), [Event::Wake]);
        drop(waking);
        WOKEN.store(false, Ordering::SeqCst);
        waker.wake();
        assert!(WAKE.lock().unwrap().is_none());

        // not woken before it starts: nothing to deliver
        WOKEN.store(false, Ordering::SeqCst);
        let (tx3, rx3) = mpsc::channel();
        drop(Waking::listen(tx3));
        assert!(rx3.try_recv().is_err());
    }

    #[test]
    fn a_wake_as_the_app_starts_is_never_lost() {
        let _wakes = WAKES.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..200 {
            WOKEN.store(false, Ordering::SeqCst);
            let (tx, rx) = mpsc::channel();
            let woke = std::thread::spawn(|| Waker::new().wake());
            let waking = Waking::listen(tx);
            woke.join().unwrap();
            assert_eq!(rx.try_iter().count(), 1);
            drop(waking);
        }
        WOKEN.store(false, Ordering::SeqCst);
    }

    /// Held by tests that set the panic hook, which is the whole process's.
    static HOOKS: Mutex<()> = Mutex::new(());

    #[test]
    fn the_panic_hook_is_put_back_unless_another_was_set_since() {
        let _hooks = HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
        let theirs: PanicHook = Box::new(|_| {});
        let theirs_at = hook_address(&theirs);
        let before = std::panic::take_hook();
        std::panic::set_hook(theirs);

        drop(Hook::install(|_| true));
        let now = std::panic::take_hook();
        assert_eq!(hook_address(&now), theirs_at, "theirs again, exactly");
        std::panic::set_hook(now);

        let hook = Hook::install(|_| true);
        let later: PanicHook = Box::new(|_| {});
        let later_at = hook_address(&later);
        std::panic::set_hook(later);
        drop(hook);
        assert_eq!(hook_address(&std::panic::take_hook()), later_at, "the newer one stays");
        std::panic::set_hook(before);
    }

    #[test]
    fn the_reader_turns_bytes_into_events_and_pauses_into_esc() {
        let mut script = reads(vec![
            Ok(b"a\x1b"),
            Ok(b""),
            Ok(b"\x1b["),
            Ok(b"A"),
            Err(io::Error::other("closed")),
        ]);
        let (tx, rx) = mpsc::channel();
        listen(&mut script, &tx, &Control::default());
        let got: Vec<Typed> = rx.try_iter().collect();
        let (closed, got) = got.split_last().unwrap();
        let got: Vec<&Event> = got.iter().map(|e| e.as_ref().unwrap()).collect();
        assert_eq!(
            got,
            [&Event::Key(Key::Char('a')), &Event::Key(Key::Esc), &Event::Key(Key::Up)]
        );
        // and then says it can read no more
        assert_eq!(closed.as_ref().unwrap_err().to_string(), "closed");
    }

    #[test]
    fn the_reader_stops_when_told_or_when_nobody_listens() {
        let (tx, _rx) = mpsc::channel();
        let stopped = Control::default();
        stopped.stop.store(true, Ordering::SeqCst);
        // one reader for both, so it can count its reads
        let mut reads = 0;
        let mut read = |buf: &mut [u8]| {
            reads += 1;
            buf[0] = b'q';
            Ok(1)
        };
        listen(&mut read, &tx, &stopped);

        let (tx, rx) = mpsc::channel();
        drop(rx);
        listen(&mut read, &tx, &Control::default());
        assert_eq!(reads, 1, "nothing read after stop, then one key nobody took");
    }

    #[test]
    fn a_paused_reader_leaves_the_keyboard_alone() {
        let control = Arc::new(Control::default());
        control.pause.store(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        let reader = {
            let control = control.clone();
            std::thread::spawn(move || {
                listen(
                    &mut |buf: &mut [u8]| {
                        buf[0] = b'x';
                        Ok(1)
                    },
                    &tx,
                    &control,
                )
            })
        };
        while !control.idle.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(30));
        assert!(rx.try_recv().is_err(), "nothing read while paused");
        control.pause.store(false, Ordering::SeqCst);
        assert_eq!(rx.recv().unwrap().unwrap(), Event::Key(Key::Char('x')));
        control.stop.store(true, Ordering::SeqCst);
        reader.join().unwrap();
    }

    #[test]
    fn suspending_outside_an_app_just_runs_the_closure_and_the_screen_is_redrawn_after() {
        assert_eq!(suspend(|| 7).unwrap(), 7);

        // after a suspend, every row is sent again, changed or not
        struct Suspends(usize);
        impl App for Suspends {
            fn view(&self, frame: &mut Frame) {
                frame.print(frame.area(), 0, "same");
                frame.animate();
            }
            fn event(&mut self, event: &Event) -> Flow {
                if *event == Event::Key(Key::Char('s')) {
                    REDRAW.with(|r| r.set(true));
                }
                self.0 += usize::from(!matches!(event, Event::Resize { .. }));
                if self.0 == 2 { Flow::Quit } else { Flow::Continue }
            }
            fn fps(&self) -> u32 {
                240
            }
        }
        let rx = keys(&[Event::Key(Key::Char('s'))]);
        let mut out = Vec::new();
        drive(&mut Suspends(0), &rx, &mut out, &mut || (10, 1)).unwrap();
        assert_eq!(String::from_utf8(out).unwrap().matches("same").count(), 2);
    }

    #[test]
    fn navigation_covers_every_key() {
        use MouseKind::*;
        let scroll = |kind| Event::Mouse(Mouse::new(kind, 0, 0));
        let mut at = 5;
        let steps = [
            (Event::Key(Key::Up), 4),
            (Event::Key(Key::Char('k')), 3),
            (Event::Key(Key::Down), 4),
            (Event::Key(Key::Char('j')), 5),
            (Event::Key(Key::PageDown), 15),
            (Event::Key(Key::PageDown), 19),
            (Event::Key(Key::PageUp), 9),
            (Event::Key(Key::Home), 0),
            (Event::Key(Key::Char('G')), 19),
            (Event::Key(Key::Char('g')), 0),
            (scroll(ScrollDown), 1),
            (scroll(ScrollUp), 0),
            (Event::Key(Key::End), 19),
        ];
        for (event, want) in steps {
            navigate(&mut at, 20, &event, Keymap::Vim);
            assert_eq!(at, want, "after {event:?}");
        }
        assert!(!navigate(&mut at, 20, &Event::Key(Key::End), Keymap::Vim));
        assert!(!navigate(&mut at, 20, &Event::Tick, Keymap::Vim));
        assert!(!navigate(&mut at, 20, &scroll(Click), Keymap::Vim));
        let mut empty = 0;
        assert!(!navigate(&mut empty, 0, &Event::Key(Key::Down), Keymap::Vim));
        // letters are left alone with the arrows alone
        for c in ['j', 'k', 'g', 'G'] {
            assert!(!navigate(&mut at, 20, &Event::Key(Key::Char(c)), Keymap::Arrows));
        }
        assert!(navigate(&mut at, 20, &Event::Key(Key::Up), Keymap::Arrows));
        // a picker's keys leave the letters for typing
        let mut at = 5;
        for (key, want) in [
            (Key::Ctrl('n'), 6),
            (Key::Tab, 7),
            (Key::Ctrl('j'), 8),
            (Key::Ctrl('p'), 7),
            (Key::BackTab, 6),
            (Key::Ctrl('k'), 5),
            (Key::PageDown, 15),
        ] {
            navigate(&mut at, 20, &Event::Key(key), Keymap::Picker);
            assert_eq!(at, want, "after {key:?}");
        }
        assert!(!navigate(&mut at, 20, &Event::Key(Key::Char('j')), Keymap::Picker));
        assert!(!navigate(&mut at, 20, &Event::Key(Key::Tab), Keymap::Vim));
    }

    #[test]
    fn the_default_app_quits_on_q_and_esc() {
        struct Plain;
        impl App for Plain {
            fn view(&self, _: &mut Frame) {}
        }
        let mut app = Plain;
        assert_eq!(app.event(&Event::Key(Key::Char('q'))), Flow::Quit);
        assert_eq!(app.event(&Event::Key(Key::Esc)), Flow::Quit);
        assert_eq!(app.event(&Event::Tick), Flow::Continue);
        assert_eq!((app.fps(), app.mouse()), (crate::fps(), false));
        app.view(&mut Frame::new(1, 1));
    }

    #[test]
    fn the_reader_waits_out_a_pause_and_stops_when_no_one_listens() {
        let control = Arc::new(Control::default());
        control.pause.store(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<Typed>();
        let unpause = control.clone();
        let lifted = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            assert!(unpause.idle.load(Ordering::SeqCst), "paused, so not reading");
            unpause.pause.store(false, Ordering::SeqCst);
        });
        // a byte that's no key, a key, then nobody to send the next one to
        let mut script = vec![&[0x1c][..], b"a", b"b"].into_iter();
        let mut rx = Some(rx);
        listen(
            &mut |buf: &mut [u8]| {
                let next = script.next().ok_or(io::Error::other("done"))?;
                if next == b"b" {
                    assert_eq!(
                        rx.take().unwrap().try_recv().unwrap().unwrap(),
                        Event::Key(Key::Char('a'))
                    );
                }
                buf[..next.len()].copy_from_slice(next);
                Ok(next.len())
            },
            &tx,
            &control,
        );
        lifted.join().unwrap();
        assert!(script.next().is_none());
    }

    #[test]
    fn off_a_terminal_an_app_prints_one_frame() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = crate::output::capture(|| {
            let app = crate::output::with_terminal(false, || run(Recorder::default())).unwrap();
            // told its size first, as on a terminal
            assert_eq!(
                app.seen,
                [Event::Resize {
                    width: term::width(),
                    height: term::height()
                }]
            );
        });
        assert!(out.starts_with("seen 1"), "{out:?}");
    }

    #[test]
    fn only_one_app_runs_at_a_time() {
        let only = Only::claim().unwrap();
        // refused before the terminal is touched, so this is safe on a real one
        let nested = crate::output::with_terminal(true, || run(Recorder::default()));
        let err = nested.err().expect("a second app shouldn't start");
        assert_eq!(err.to_string(), "an app is already running");
        assert!(Only::claim().is_err());
        drop(only);
        assert!(Only::claim().is_ok());
    }

    #[test]
    fn a_panic_on_another_thread_is_kept_until_the_screen_is_left() {
        let _hooks = HOOKS.lock().unwrap_or_else(PoisonError::into_inner);
        static HANDED_ON: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        const MARK: &str = "tinsel test: a worker broke";
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
            if info.payload().downcast_ref::<&str>().is_some_and(|s| s.contains(MARK)) {
                HANDED_ON.fetch_add(1, Ordering::SeqCst);
            }
        }));
        let owner = std::thread::current().id();
        let hook = Hook::install(move |info| on_panic(info, owner));
        let worker = || {
            std::thread::spawn(|| panic!("tinsel test: a worker broke"))
                .join()
                .unwrap_err()
        };

        // while an app has the screen, the message is kept rather than printed over it
        ON_SCREEN.store(true, Ordering::SeqCst);
        worker();
        ON_SCREEN.store(false, Ordering::SeqCst);
        let on_screen = HANDED_ON.load(Ordering::SeqCst);
        // and with no app on the screen, printed as usual
        worker();
        let off_screen = HANDED_ON.load(Ordering::SeqCst) - on_screen;
        drop(hook);
        std::panic::set_hook(before);

        assert_eq!((on_screen, off_screen), (0, 1));
        let mut kept = KEPT.lock().unwrap();
        let ours: Vec<String> = kept.iter().filter(|t| t.contains(MARK)).cloned().collect();
        kept.retain(|t| !t.contains(MARK));
        assert_eq!(ours.len(), 1, "{ours:?}");
        assert!(
            // file!() and not a written path, which Windows spells with backslashes
            ours[0].starts_with(&format!("thread '<unnamed>' panicked at {}:", file!())),
            "{ours:?}"
        );
        assert!(ours[0].contains(&format!(":\n{MARK}\n")), "{ours:?}");
    }

    #[test]
    fn a_redraw_from_another_thread_wakes_the_app_to_send_everything_again() {
        let _wakes = WAKES.lock().unwrap_or_else(PoisonError::into_inner);
        WOKEN.store(false, Ordering::SeqCst);
        /// Says when it's drawn, and quits when woken.
        struct Shows(mpsc::Sender<()>);
        impl App for Shows {
            fn view(&self, frame: &mut Frame) {
                frame.print(frame.area(), 0, "same");
                let _ = self.0.send(());
            }
            fn event(&mut self, event: &Event) -> Flow {
                if *event == Event::Wake {
                    Flow::Quit
                } else {
                    Flow::Continue
                }
            }
        }
        // as the app's own thread is while it runs
        RUNNING.with(|r| {
            *r.borrow_mut() = Some(Running {
                raw: None,
                control: Arc::default(),
                mouse: false,
            })
        });
        let (tx, rx) = mpsc::channel();
        let waking = Waking::listen(tx);
        let (drawn_tx, drawn) = mpsc::channel();
        let other = std::thread::spawn(move || {
            let wait = Duration::from_secs(5);
            drawn.recv_timeout(wait).unwrap();
            redraw();
            let again = drawn.recv_timeout(wait).is_ok();
            Waker::new().wake();
            again
        });
        let mut out = Vec::new();
        drive(&mut Shows(drawn_tx), &rx, &mut out, &mut || (4, 1)).unwrap();
        drop(waking);
        RUNNING.with(|r| r.borrow_mut().take());
        WOKEN.store(false, Ordering::SeqCst);
        assert!(other.join().unwrap(), "drawn again");
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out.matches("same").count(), 2, "{out:?}");
    }

    #[test]
    fn the_loop_ends_if_the_terminal_cant_be_taken_back() {
        struct Suspends(usize);
        impl App for Suspends {
            fn view(&self, frame: &mut Frame) {
                frame.print(frame.area(), 0, "shown");
            }
            fn event(&mut self, event: &Event) -> Flow {
                // as `suspend` does when it can't
                if *event == Event::Key(Key::Char('s')) {
                    lose(io::Error::other("no terminal"));
                }
                self.0 += 1;
                Flow::Continue
            }
        }
        let rx = keys(&[Event::Key(Key::Char('s')), Event::Key(Key::Char('x'))]);
        let mut out = Vec::new();
        let mut app = Suspends(0);
        let err = drive(&mut app, &rx, &mut out, &mut || (10, 1)).unwrap_err();
        assert_eq!(err.to_string(), "no terminal");
        // not drawn again after
        assert_eq!(String::from_utf8(out).unwrap().matches("shown").count(), 1);
        assert!(LOST.with(|l| l.borrow().is_none()));
    }

    #[cfg(unix)]
    #[test]
    fn ctrl_c_is_passed_on_while_suspended() {
        unsafe extern "C" {
            fn raise(sig: i32) -> i32;
        }
        // drawing tests elsewhere set their own Ctrl-C handler; capturing keeps them apart
        crate::output::capture(|| {
            let pass = interrupt::Pass::new();
            // SAFETY: SIGINT goes to `pass`, which does nothing
            assert_eq!(unsafe { raise(2) }, 0);
            drop(pass);
        });
        assert_eq!(suspend(|| 7).unwrap(), 7);
    }

    #[test]
    fn the_app_clock_runs_while_it_does() {
        struct Timed(f64);
        impl App for Timed {
            fn view(&self, _: &mut Frame) {}
            fn event(&mut self, _: &Event) -> Flow {
                self.0 = time();
                Flow::Quit
            }
        }
        let mut app = Timed(-1.0);
        drive(&mut app, &keys(&[Event::Tick]), &mut Vec::new(), &mut || (1, 1)).unwrap();
        assert!(app.0 >= 0.0);
    }

    /// Counts its frames and what it's sent, and asks for frames as told.
    #[derive(Default)]
    struct Asking {
        frames: std::cell::Cell<usize>,
        seen: Vec<Event>,
        ambient: bool,
        at: Option<f64>,
    }

    impl App for Asking {
        fn view(&self, frame: &mut Frame) {
            self.frames.set(self.frames.get() + 1);
            if self.ambient {
                frame.animate_at(200);
            }
            if let Some(at) = self.at {
                frame.redraw_at(at);
            }
        }
        fn event(&mut self, event: &Event) -> Flow {
            self.seen.push(event.clone());
            if *event == Event::Key(Key::Char('q')) {
                Flow::Quit
            } else {
                Flow::Continue
            }
        }
        fn fps(&self) -> u32 {
            200
        }
    }

    /// Events sent from another thread, each after a pause.
    fn later(script: Vec<(u64, Event)>) -> mpsc::Receiver<Typed> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (ms, event) in script {
                std::thread::sleep(Duration::from_millis(ms));
                let _ = tx.send(Ok(event));
            }
            std::thread::sleep(Duration::from_secs(5));
        });
        rx
    }

    #[test]
    fn an_app_that_asks_for_nothing_is_drawn_only_when_something_happens() {
        let mut app = Asking::default();
        let rx = later(vec![
            (150, Event::Key(Key::Char('x'))),
            (150, Event::Key(Key::Char('q'))),
        ]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        // once to start, once for the x; no ticks in all that time
        assert_eq!(app.frames.get(), 2);
        assert!(!app.seen.contains(&Event::Tick), "{:?}", app.seen);
    }

    #[test]
    fn ambient_frames_rest_while_the_terminal_is_in_the_background() {
        let mut app = Asking {
            ambient: true,
            ..Asking::default()
        };
        let rx = later(vec![(0, Event::Focus(false)), (200, Event::Key(Key::Char('q')))]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        // drawn to start, perhaps a few times before focus went, and once for losing it
        assert!(app.frames.get() <= 4, "{}", app.frames.get());

        let mut app = Asking {
            ambient: true,
            ..Asking::default()
        };
        let rx = later(vec![(200, Event::Key(Key::Char('q')))]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        assert!(app.frames.get() > 10, "{}", app.frames.get());
        assert!(app.seen.contains(&Event::Tick));
    }

    #[test]
    fn a_moment_asked_for_comes_in_the_background_too() {
        let mut app = Asking {
            at: Some(0.1),
            ..Asking::default()
        };
        let rx = later(vec![(0, Event::Focus(false)), (300, Event::Key(Key::Char('q')))]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        let ticks = app.seen.iter().filter(|e| **e == Event::Tick).count();
        // asked for again in every frame, but a moment gone is due only until it's been drawn
        assert!(ticks >= 1, "{:?}", app.seen);
    }

    #[test]
    fn the_frame_knows_the_ambient_clock() {
        struct Clocked(std::cell::Cell<(f64, f64)>);
        impl App for Clocked {
            fn view(&self, frame: &mut Frame) {
                self.0.set((frame.time(), frame.ambient_time()));
            }
            fn event(&mut self, event: &Event) -> Flow {
                if *event == Event::Key(Key::Char('q')) {
                    Flow::Quit
                } else {
                    Flow::Continue
                }
            }
        }
        let mut app = Clocked(std::cell::Cell::new((0.0, 0.0)));
        let rx = later(vec![
            (0, Event::Focus(false)),
            (150, Event::Key(Key::Char('x'))),
            (50, Event::Key(Key::Char('q'))),
        ]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (4, 1)).unwrap();
        let (time, ambient) = app.0.get();
        assert!(time - ambient >= 0.1, "{time} {ambient}");
    }

    #[test]
    fn the_window_colour_is_sent_again_once_settled_and_on_coming_back() {
        struct Coloured;
        impl App for Coloured {
            fn view(&self, frame: &mut Frame) {
                frame.set_background(crate::Rgb(1, 2, 3));
            }
            fn event(&mut self, event: &Event) -> Flow {
                if *event == Event::Key(Key::Char('q')) {
                    Flow::Quit
                } else {
                    Flow::Continue
                }
            }
        }
        crate::with_color_mode(crate::ColorMode::TrueColor, || {
            let rx = later(vec![
                (700, Event::Key(Key::Char('x'))),
                (50, Event::Focus(false)),
                (50, Event::Focus(true)),
                (50, Event::Key(Key::Char('q'))),
            ]);
            let mut out = Vec::new();
            drive(&mut Coloured, &rx, &mut out, &mut || (4, 1)).unwrap();
            let out = String::from_utf8(out).unwrap();
            // to start, once settled, and on coming back; not for the key or for leaving
            assert_eq!(out.matches("\x1b]11;").count(), 3, "{out:?}");
        });
    }

    #[test]
    fn the_size_is_known_from_the_start_and_forgotten_after() {
        struct Sizes(Vec<(usize, usize)>);
        impl App for Sizes {
            fn view(&self, _: &mut Frame) {}
            fn event(&mut self, event: &Event) -> Flow {
                self.0.push(size());
                if matches!(event, Event::Key(_)) {
                    Flow::Quit
                } else {
                    Flow::Continue
                }
            }
        }
        let mut app = Sizes(Vec::new());
        let rx = keys(&[Event::Key(Key::Char('x'))]);
        drive(&mut app, &rx, &mut Vec::new(), &mut || (12, 3)).unwrap();
        assert_eq!(app.0, [(12, 3), (12, 3)]);
        assert!(SIZE.with(std::cell::Cell::get).is_none());
    }
}

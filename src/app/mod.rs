//! Full-screen terminal apps: dashboards, pickers, little toys, in the same pink as
//! everything else.
//!
//! An app is a struct with your state, and two methods: [`view`](App::view) draws the state
//! into a [`Frame`], and [`event`](App::event) changes it when a key is pressed, the mouse
//! moves or time passes. [`run`] takes over the screen, calls them in turn, and puts the
//! terminal back when you're done (or if anything panics).
//!
//! ```no_run
//! use tinsel::app::{self, App, Event, Flow, Frame, Key, List, Panel, Size::*, hints};
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
//!         app::navigate(&mut self.selected, self.flavours.len(), event);
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
//! [`Tabs`], [`Plot`], [`Stat`], [`big`] text, [`hints`] for the footer and [`Sparkles`]
//! for the background.
//!
//! # Time
//!
//! [`Frame::time`] is the seconds since the app started, for anything animated, and
//! [`Event::Tick`] arrives [`fps`](App::fps) times a second, for polling whatever your
//! dashboard shows.
//!
//! # Tests
//!
//! [`snapshot`] draws one frame of an app at any size, without a terminal, so you can check
//! exactly what it shows. When stdout isn't a terminal, [`run`] prints that one frame and
//! returns.

mod frame;
mod input;
mod layout;
mod widgets;

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub use frame::Frame;
pub use input::{Key, Mouse, MouseKind};
pub use layout::{Rect, Size};
pub use widgets::{List, Panel, Plot, Sparkles, Stat, Tabs, Widget, big, hints};

use crate::block::Block;
use crate::term;

/// Something that happened, handed to [`App::event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A key was pressed.
    Key(Key),
    /// The mouse was clicked or scrolled (only if [`App::mouse`] says so).
    Mouse(Mouse),
    /// Time passed: [`App::fps`] of these arrive a second.
    Tick,
    /// The terminal changed size, to this many columns and rows.
    Resize(usize, usize),
}

/// Whether to keep going after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Carry on.
    Continue,
    /// Put the terminal back and return from [`run`].
    Quit,
}

/// A full-screen app. Only [`view`](App::view) is needed: by default `q` and Esc quit,
/// it draws at 30 frames a second, and the mouse is left alone.
pub trait App {
    /// Draw the app as it is now. Called every frame, so keep it quick, and keep slow work
    /// (fetching, reading files) in [`event`](App::event) or another thread.
    fn view(&self, frame: &mut Frame);

    /// Change the app in response to something happening. Ctrl-C always quits, whatever
    /// this returns.
    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(Key::Char('q') | Key::Esc) => Flow::Quit,
            _ => Flow::Continue,
        }
    }

    /// How many times a second to draw and send [`Event::Tick`]. 30 unless changed; lower
    /// it for a dashboard with nothing animated.
    fn fps(&self) -> u32 {
        30
    }

    /// Whether to report clicks and the scroll wheel. Off unless changed, since while it's on
    /// the terminal can't select text with the mouse.
    fn mouse(&self) -> bool {
        false
    }
}

/// Run an app full-screen until it quits, then hand it back, so you can read what was chosen.
///
/// The screen is put back as it was afterwards, whether the app quits, returns an error or
/// panics. When stdout isn't a terminal, one frame is printed instead, at the terminal's
/// size (or 80×24), and the app is returned straight away.
pub fn run<A: App>(mut app: A) -> io::Result<A> {
    if !term::interactive() {
        snapshot(&app, term::width(), term::height()).print();
        return Ok(app);
    }
    // decide these before stdin is in raw mode: finding out may mean asking the terminal
    crate::style::theme();
    crate::style::color_mode();

    let session = Session::enter(app.mouse())?;
    let (tx, rx) = mpsc::channel();
    let stop = session.stop.clone();
    let reader = std::thread::spawn(move || listen(&tx, &stop));

    let start = Instant::now();
    let tick = Duration::from_secs_f64(1.0 / app.fps().clamp(1, 240) as f64);
    let mut next_tick = start + tick;
    let mut size = (term::width(), term::height());
    let mut shown: Option<Frame> = None;
    let mut out = io::stdout();
    'run: loop {
        let now_size = (term::width(), term::height());
        if now_size != size {
            size = now_size;
            shown = None;
            if app.event(&Event::Resize(size.0, size.1)) == Flow::Quit {
                break;
            }
        }
        let mut frame = Frame::new(size.0, size.1).at(start.elapsed().as_secs_f32());
        app.view(&mut frame);
        let diff = frame.diff(shown.as_ref());
        if !diff.is_empty() {
            write!(out, "\x1b[?2026h{diff}\x1b[?2026l")?;
            out.flush()?;
        }
        shown = Some(frame);

        let wait = next_tick.saturating_duration_since(Instant::now());
        let first = match rx.recv_timeout(wait) {
            Ok(event) => Some(event),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            // the reader stopped: stdin is gone, so nothing more can be typed
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        for event in first.into_iter().chain(rx.try_iter()) {
            if event == Event::Key(Key::Ctrl('c')) || app.event(&event) == Flow::Quit {
                break 'run;
            }
        }
        let now = Instant::now();
        if now >= next_tick {
            // after a stall, carry on from now rather than sending a burst of ticks
            next_tick = (next_tick + tick).max(now);
            if app.event(&Event::Tick) == Flow::Quit {
                break;
            }
        }
    }
    // the reader has to finish while reads still time out: once the terminal is back in line
    // mode, a read waits for Enter
    session.stop.store(true, Ordering::Relaxed);
    let _ = reader.join();
    drop(session);
    Ok(app)
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

/// Move a selection through a list of `len` things in response to an event: ↑↓ and j/k one
/// at a time, Page Up/Down ten, Home/End (or g/G) to the ends, and the scroll wheel. Returns
/// whether it moved. Made for [`List`], but works for anything with a position.
///
/// ```
/// use tinsel::app::{Event, Key, navigate};
///
/// let mut selected = 0;
/// navigate(&mut selected, 3, &Event::Key(Key::Down));
/// navigate(&mut selected, 3, &Event::Key(Key::End));
/// assert_eq!(selected, 2);
/// ```
pub fn navigate(selected: &mut usize, len: usize, event: &Event) -> bool {
    let last = len.saturating_sub(1);
    let to = match event {
        Event::Key(Key::Up | Key::Char('k')) => selected.saturating_sub(1),
        Event::Key(Key::Down | Key::Char('j')) => *selected + 1,
        Event::Key(Key::PageUp) => selected.saturating_sub(10),
        Event::Key(Key::PageDown) => *selected + 10,
        Event::Key(Key::Home | Key::Char('g')) => 0,
        Event::Key(Key::End | Key::Char('G')) => last,
        Event::Mouse(m) if m.kind == MouseKind::ScrollUp => selected.saturating_sub(1),
        Event::Mouse(m) if m.kind == MouseKind::ScrollDown => *selected + 1,
        _ => return false,
    };
    let to = to.min(last);
    let moved = to != *selected;
    *selected = to;
    moved
}

// MARK: - The terminal

/// Whether an app has the screen, so a panic can give it back before its message is printed.
static ON_SCREEN: AtomicBool = AtomicBool::new(false);

const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J";
const LEAVE: &str = "\x1b[0m\x1b[?1000l\x1b[?1006l\x1b[?7h\x1b[?25h\x1b[?1049l";
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1006h";

/// The alternate screen, raw keys and a hidden cursor, for as long as it lives.
struct Session {
    _raw: term::RawMode,
    stop: Arc<AtomicBool>,
    hook: Arc<PanicHook>,
}

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

impl Session {
    fn enter(mouse: bool) -> io::Result<Session> {
        term::enable_escapes();
        term::remember_terminal();
        let raw = term::RawMode::enter()?;
        // give the screen back before any panic message, which would otherwise be drawn on
        // the alternate screen and vanish with it
        let hook: Arc<PanicHook> = Arc::new(std::panic::take_hook());
        let previous = hook.clone();
        std::panic::set_hook(Box::new(move |info| {
            leave_screen();
            term::restore_saved_terminal();
            previous(info);
        }));
        ON_SCREEN.store(true, Ordering::SeqCst);
        let mut out = io::stdout();
        write!(out, "{ENTER}{}", if mouse { MOUSE_ON } else { "" })?;
        out.flush()?;
        Ok(Session {
            _raw: raw,
            stop: Arc::new(AtomicBool::new(false)),
            hook,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        leave_screen();
        term::forget_terminal();
        if !std::thread::panicking() {
            let previous = self.hook.clone();
            let _ = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| previous(info)));
        }
    }
}

fn leave_screen() {
    if ON_SCREEN.swap(false, Ordering::SeqCst) {
        let mut out = io::stdout();
        let _ = out.write_all(LEAVE.as_bytes());
        let _ = out.flush();
    }
}

/// Read keys and mouse reports until told to stop, sending each as an event.
fn listen(tx: &mpsc::Sender<Event>, stop: &AtomicBool) {
    let mut pending: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // reads give up after 0.1s, so `stop` is checked often and a pause can end a sequence
        let paused = match term::read_byte() {
            Ok(Some(b)) => {
                pending.push(b);
                false
            }
            Ok(None) => true,
            Err(_) => return,
        };
        while let Some((event, used)) = input::parse(&pending, paused) {
            pending.drain(..used);
            if let Some(event) = event {
                if tx.send(event).is_err() {
                    return;
                }
            }
        }
    }
}

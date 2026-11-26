//! Full-screen apps on a web page, with the `web` feature: the app built for wasm, and drawn
//! in the browser as [`Frame::html_pre`](crate::app::Frame::html_pre) draws it, the keys, the
//! mouse and the window's size passed in. [`web_app!`](crate::web_app) names the app, in a
//! crate built as a `cdylib`:
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! tinsel = { version = "1.2", features = ["web"] }
//! ```
//!
//! ```ignore
//! tinsel::web_app!(Counter::default());
//! ```
//!
//! Built with `cargo build --release --target wasm32-wasip1`, it's run in a page by
//! [`SCRIPT`], which a page imports:
//!
//! ```html
//! <div id="app" style="height: 100vh"></div>
//! <script type="module">
//!   import { run } from './tinsel.js';
//!   run(document.getElementById('app'), 'counter.wasm');
//! </script>
//! ```
//!
//! The app runs as it would in a terminal: told its size first, sent [`Event::Tick`] when a
//! frame it asked for comes due, and drawn only when something changes, with
//! [`app::time`](crate::app::time) and [`app::size`](crate::app::size) as they'd be there.
//! What a page can't do, it doesn't: there are no threads, so no [`Job`](crate::app::Job) or
//! [`Waker`](crate::app::Waker); no files, processes or clipboard; and pictures aren't shown.

use std::cell::RefCell;
use std::time::Instant;

use crate::app::schedule::{self, Ambient};
use crate::app::{App, Clock, Event, Flow, Frame, Key, Measured, Mouse, MouseKind};

/// The page's side, as an ES module: it loads the app's wasm, gives it the little of WASI it
/// needs (a clock, randomness and an empty environment), passes in the keys, the mouse, the
/// window's size and whether it's in view, and puts each frame in place, a row at a time.
/// Write it next to the page, as `tinsel.js` say, and call its `run(element, wasm, options)`;
/// the options are `light` for tinsel's light colours, `fontSize` in pixels, and `onQuit`,
/// called with the browser's name for the key that quit the app (`"q"`, `"Escape"`).
pub const SCRIPT: &str = include_str!("web.js");

/// Whether the page is on a touch screen, with no keyboard to speak of: for an app to leave
/// out the keys' hints a touch screen has no use for. Known once [`web_app!`](crate::web_app)
/// is starting the app, so it can be asked while the app is made.
pub fn touch() -> bool {
    TOUCH.with(std::cell::Cell::get)
}

/// Run `app` on a web page: makes the functions the page's [`SCRIPT`] calls, in a crate built
/// as a `cdylib` for wasm. `app` is made as the page starts, after [`touch`] is known.
///
/// ```ignore
/// tinsel::web_app!(Counter::default());
/// ```
#[macro_export]
macro_rules! web_app {
    ($app:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_start(width: usize, height: usize, light: u32, touch: u32) {
            $crate::web::__start(width, height, light != 0, touch != 0, || {
                ::std::boxed::Box::new($app)
            });
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_resize(width: usize, height: usize) {
            $crate::web::__resize(width, height)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_key(kind: u32, ch: u32) -> u32 {
            $crate::web::__key(kind, ch)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_mouse(kind: u32, x: usize, y: usize) -> u32 {
            $crate::web::__mouse(kind, x, y)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_focus(on: u32) {
            $crate::web::__focus(on != 0)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_frame() -> *const u8 {
            $crate::web::__frame()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_frame_len() -> usize {
            $crate::web::__frame_len()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_changed() -> u32 {
            $crate::web::__changed()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_panic() -> *const u8 {
            $crate::web::__panic()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn tinsel_panic_len() -> usize {
            $crate::web::__panic_len()
        }
    };
}

struct Running {
    app: Box<dyn App>,
    start: Instant,
    size: (usize, usize),
    ambient: Ambient,
    // when the frame asked for comes due, on the app's clock; None while nothing was asked for
    due: Option<f64>,
    // something happened since the last frame, so the next is drawn whenever it's asked for
    dirty: bool,
    // the last frame, for the page to read out of memory, and whether it differs from the one
    // before, which the page needn't put in place again
    drawn: String,
    changed: bool,
    _clock: Clock,
    _size: Measured,
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    static TOUCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with<T>(f: impl FnOnce(&mut Running) -> T) -> T {
    RUNNING.with(|r| f(r.borrow_mut().as_mut().expect("the page starts the app first")))
}

// an event, and whether the app quit on it, as Ctrl-C quits it whatever it says unless it says
// otherwise, the way the terminal's loop has it
fn send(r: &mut Running, event: &Event) -> bool {
    r.dirty = true;
    let quit = r.app.event(event) == Flow::Quit;
    quit || (*event == Event::Key(Key::Ctrl('c')) && r.app.ctrl_c_quits())
}

#[doc(hidden)]
pub fn __start(width: usize, height: usize, light: bool, touch: bool, app: impl FnOnce() -> Box<dyn App>) {
    std::panic::set_hook(Box::new(|info| PANIC.with(|p| *p.borrow_mut() = info.to_string())));
    crate::set_color_mode(crate::ColorMode::TrueColor);
    crate::set_theme(if light { crate::Theme::Light } else { crate::Theme::Dark });
    TOUCH.with(|t| t.set(touch));
    let start = Instant::now();
    let mut running = Running {
        app: app(),
        start,
        size: (width, height),
        ambient: Ambient::default(),
        due: None,
        dirty: true,
        drawn: String::new(),
        changed: false,
        _clock: Clock::start(start),
        _size: Measured::start((width, height)),
    };
    // told before the first frame, so it can lay itself out before it's drawn
    send(&mut running, &Event::Resize { width, height });
    RUNNING.with(|r| *r.borrow_mut() = Some(running));
}

#[doc(hidden)]
pub fn __resize(width: usize, height: usize) {
    with(|r| {
        if r.size == (width, height) {
            return;
        }
        r.size = (width, height);
        Measured::start(r.size);
        send(r, &Event::Resize { width, height });
    });
}

/// A key, as `kind`: 0 a character, 1 Ctrl and 2 Alt with one, then Enter, Tab, Shift-Tab,
/// Backspace, Delete, Esc, the arrows (up, down, left, right), Home, End, Page Up and Page
/// Down. 1 when the app quit on it.
#[doc(hidden)]
pub fn __key(kind: u32, ch: u32) -> u32 {
    // a character that isn't one is dropped, rather than pressed as something else
    let Some(c) = char::from_u32(ch) else { return 0 };
    let key = match kind {
        0 => Key::Char(c),
        1 => Key::Ctrl(c),
        2 => Key::Alt(c),
        3 => Key::Enter,
        4 => Key::Tab,
        5 => Key::BackTab,
        6 => Key::Backspace,
        7 => Key::Delete,
        8 => Key::Esc,
        9 => Key::Up,
        10 => Key::Down,
        11 => Key::Left,
        12 => Key::Right,
        13 => Key::Home,
        14 => Key::End,
        15 => Key::PageUp,
        16 => Key::PageDown,
        _ => return 0,
    };
    with(|r| send(r, &Event::Key(key))) as u32
}

/// The mouse at cell `x`, `y`, as `kind`: 0 a click, 1 a release, 2 a drag, 3 the wheel up and
/// 4 down. 1 when the app quit on it.
#[doc(hidden)]
pub fn __mouse(kind: u32, x: usize, y: usize) -> u32 {
    let kind = match kind {
        0 => MouseKind::Click,
        1 => MouseKind::Release,
        2 => MouseKind::Drag,
        3 => MouseKind::ScrollUp,
        4 => MouseKind::ScrollDown,
        _ => return 0,
    };
    with(|r| send(r, &Event::Mouse(Mouse::new(kind, x, y)))) as u32
}

/// The page came into view, or went out of it, as a terminal gains and loses focus.
#[doc(hidden)]
pub fn __focus(on: bool) {
    with(|r| {
        let now = r.start.elapsed().as_secs_f64();
        r.ambient.focus(on, now);
        send(r, &Event::Focus(on));
    });
}

/// The frame, drawn if something happened or the one asked for is due, else the last one
/// again; [`__changed`] says which.
#[doc(hidden)]
pub fn __frame() -> *const u8 {
    with(|r| {
        let now = r.start.elapsed().as_secs_f64();
        let due = r.due.is_some_and(|due| now >= due);
        r.changed = false;
        if !r.dirty && !due {
            return r.drawn.as_ptr();
        }
        // the frame asked for is due: time passed, as far as the app is concerned
        if due && !r.dirty {
            r.app.event(&Event::Tick);
        }
        r.dirty = false;
        let mut frame = Frame::new(r.size.0, r.size.1).at(now).ambient_at(r.ambient.at(now));
        r.app.view(&mut frame);
        r.due = schedule::next_frame(frame.wants(), now, r.app.fps(), r.ambient.focused());
        let html = frame.html_pre();
        if html != r.drawn {
            r.drawn = html;
            r.changed = true;
        }
        r.drawn.as_ptr()
    })
}

#[doc(hidden)]
pub fn __frame_len() -> usize {
    with(|r| r.drawn.len())
}

#[doc(hidden)]
pub fn __changed() -> u32 {
    with(|r| r.changed as u32)
}

#[doc(hidden)]
pub fn __panic() -> *const u8 {
    PANIC.with(|p| p.borrow().as_ptr())
}

#[doc(hidden)]
pub fn __panic_len() -> usize {
    PANIC.with(|p| p.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter(u32);

    impl App for Counter {
        fn view(&self, frame: &mut Frame) {
            frame.print(frame.area(), 0, &format!("pressed {}", self.0));
        }

        fn event(&mut self, event: &Event) -> Flow {
            match event {
                Event::Key(Key::Char('q')) => Flow::Quit,
                Event::Key(_) => {
                    self.0 += 1;
                    Flow::Continue
                }
                _ => Flow::Continue,
            }
        }
    }

    fn shown() -> String {
        let (ptr, len) = (__frame(), __frame_len());
        // SAFETY: the frame stays where it is until the next one is drawn
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
    }

    #[test]
    fn an_app_runs_as_the_page_drives_it() {
        __start(20, 2, false, true, || Box::new(Counter::default()));
        assert!(touch());
        assert!(shown().contains("pressed 0"));
        assert_eq!(__changed(), 1);
        // nothing happened, so nothing's drawn
        shown();
        assert_eq!(__changed(), 0);
        assert_eq!(__key(0, 'j' as u32), 0);
        assert!(shown().contains("pressed 1"));
        assert_eq!(__key(0, 'q' as u32), 1, "q quits it");
        assert_eq!(__key(0, 0xD800), 0, "half a character is dropped");
        assert!(shown().contains("pressed 1"));
        RUNNING.with(|r| r.borrow_mut().take());
    }
}

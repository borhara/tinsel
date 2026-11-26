//! `tinsel docs` for a web page, with the `web` feature, built for wasm:
//!
//! ```text
//! cargo rustc --lib --release --features web --target wasm32-wasip1 --crate-type cdylib
//! ```
//!
//! The page passes keys, the mouse and its size in, and shows each frame as
//! [`html_pre`](crate::app::Frame::html_pre) gives it. Time comes from WASI's clock, which the
//! page supplies, with the rest of WASI left unanswered.

use std::cell::RefCell;

use crate::app::{App, Event, Frame, Key, Mouse, MouseKind};
use crate::docs::{Docs, Tab};

// what docs::run calls for a guide that isn't there, which the page never asks for
pub(crate) fn usage_error(problem: &str) -> ! {
    panic!("{problem}")
}

struct Reader {
    docs: Docs,
    width: usize,
    height: usize,
    // the last frame drawn, for the page to read out of memory
    drawn: String,
    moving: bool,
}

thread_local! {
    static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
    static PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with<T>(f: impl FnOnce(&mut Reader) -> T) -> T {
    READER.with(|r| f(r.borrow_mut().as_mut().expect("start is called first")))
}

/// Open the reader on the first guide, `width` by `height` cells, in light colours unless
/// `light` is 0, and saying which keys do what unless `touch` isn't 0.
#[unsafe(no_mangle)]
pub extern "C" fn start(width: usize, height: usize, light: u32, touch: u32) {
    std::panic::set_hook(Box::new(|info| PANIC.with(|p| *p.borrow_mut() = info.to_string())));
    crate::set_color_mode(crate::ColorMode::TrueColor);
    crate::set_theme(if light != 0 { crate::Theme::Light } else { crate::Theme::Dark });
    let mut docs = Docs::new(Tab::Guides, 0);
    docs.hints = touch == 0;
    docs.event(&Event::Resize { width, height });
    READER.with(|r| {
        *r.borrow_mut() = Some(Reader {
            docs,
            width,
            height,
            drawn: String::new(),
            moving: true,
        })
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn resize(width: usize, height: usize) {
    with(|r| {
        (r.width, r.height) = (width, height);
        r.docs.event(&Event::Resize { width, height });
    });
}

/// A key, as `kind`: 0 a character, 1 Ctrl and 2 Alt with one, then Enter, Tab, Shift-Tab,
/// Backspace, Delete, Esc, the arrows (up, down, left, right), Home, End, Page Up and Page
/// Down. Quitting is left to closing the page.
#[unsafe(no_mangle)]
pub extern "C" fn key(kind: u32, ch: u32) {
    let c = char::from_u32(ch).unwrap_or(' ');
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
        _ => return,
    };
    with(|r| r.docs.event(&Event::Key(key)));
}

/// The mouse at cell `x`, `y`, as `kind`: 0 a click, 1 a release, 2 a drag, 3 the wheel up
/// and 4 down.
#[unsafe(no_mangle)]
pub extern "C" fn mouse(kind: u32, x: usize, y: usize) {
    let kind = match kind {
        0 => MouseKind::Click,
        1 => MouseKind::Release,
        2 => MouseKind::Drag,
        3 => MouseKind::ScrollUp,
        4 => MouseKind::ScrollDown,
        _ => return,
    };
    with(|r| r.docs.event(&Event::Mouse(Mouse::new(kind, x, y))));
}

/// Draw the reader `seconds` in, and give where the frame is: a `<pre>`, `frame_len` bytes
/// long.
#[unsafe(no_mangle)]
pub extern "C" fn frame(seconds: f64) -> *const u8 {
    with(|r| {
        let mut frame = Frame::new(r.width, r.height).at(seconds);
        r.docs.view(&mut frame);
        r.moving = frame.wants_redraw();
        r.drawn = frame.html_pre();
        r.drawn.as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn frame_len() -> usize {
    with(|r| r.drawn.len())
}

/// Whether the last frame has something moving, and wants another soon.
#[unsafe(no_mangle)]
pub extern "C" fn moving() -> u32 {
    with(|r| r.moving as u32)
}

/// How many frames a second the reader wants while something moves.
#[unsafe(no_mangle)]
pub extern "C" fn fps() -> u32 {
    with(|r| r.docs.fps())
}

/// Where the message of a panic is, if there was one, `panic_len` bytes long.
#[unsafe(no_mangle)]
pub extern "C" fn panic_message() -> *const u8 {
    PANIC.with(|p| p.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn panic_len() -> usize {
    PANIC.with(|p| p.borrow().len())
}

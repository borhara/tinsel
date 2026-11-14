//! The terminal itself: its size, its modes, its background colour, and putting everything
//! back on Ctrl-C. Calls libc directly and shells out to `stty`, to stay dependency-free.

use std::io::{self, IsTerminal, Read, Write};
use std::time::{Duration, Instant};

/// The width tinsel lays things out in: the terminal's, capped so lines stay readable.
pub fn content_width() -> usize {
    width().min(76)
}

/// Columns in the terminal tinsel draws on (stdout, or the [`Target`](crate::Target)), falling
/// back to `$COLUMNS`, then 80.
pub fn width() -> usize {
    measured_or(ioctl_size().map(|(cols, _)| cols), "COLUMNS", 80)
}

/// Rows in the terminal tinsel draws on, falling back to `$LINES`, then 24.
pub(crate) fn height() -> usize {
    measured_or(ioctl_size().map(|(_, rows)| rows), "LINES", 24)
}

/// What the terminal measured, or else what the environment variable `var` says, or else
/// `default`.
fn measured_or(measured: Option<usize>, var: &str, default: usize) -> usize {
    measured
        .or_else(|| std::env::var(var).ok()?.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// Whether both ends are a terminal: something to draw on, and someone typing.
pub(crate) fn interactive() -> bool {
    if let Some(terminal) = crate::output::pretended_terminal() {
        return terminal;
    }
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Run `stty` against the terminal on stdin. `stty` is used rather than termios structs
/// because their layout differs between operating systems.
#[cfg(not(windows))]
pub(crate) fn stty(args: &[&str]) -> io::Result<String> {
    use std::process::{Command, Stdio};
    let out = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .stderr(Stdio::null())
        .output()?;
    if !out.status.success() {
        return Err(io::Error::other("stty couldn't configure the terminal"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Character-at-a-time input with no echo, read with [`read_some`], for listening to the
/// terminal while something's drawn. Unlike [`RawMode`], Ctrl-C still interrupts. Undone
/// when dropped.
pub(crate) struct Cbreak {
    #[cfg(not(windows))]
    saved: String,
    #[cfg(windows)]
    saved: win::Saved,
}

impl Cbreak {
    pub(crate) fn enter() -> io::Result<Cbreak> {
        #[cfg(not(windows))]
        {
            let saved = stty(&["-g"])?;
            stty(&["-icanon", "-echo", "min", "0", "time", "1"])?;
            Ok(Cbreak { saved })
        }
        #[cfg(windows)]
        {
            Ok(Cbreak {
                saved: win::enter(false)?,
            })
        }
    }
}

impl Drop for Cbreak {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        let _ = stty(&[&self.saved]);
        #[cfg(windows)]
        win::leave_raw(&self.saved);
    }
}

/// Whatever's been typed, up to `buf`'s length, waiting at most 0.1s for it: 0 if nothing
/// came. For use in [`Cbreak`] or [`RawMode`].
pub(crate) fn read_some(buf: &mut [u8]) -> io::Result<usize> {
    #[cfg(windows)]
    if !win::key_waiting(100) {
        return Ok(0);
    }
    io::stdin().lock().read(buf)
}

/// Stdin read through [`read_some`], so reads give up after 0.1s everywhere.
pub(crate) struct TimedStdin;

impl Read for TimedStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        read_some(buf)
    }
}

/// Whether the terminal answers questions and reports focus (OSC 11, mode 1004). On Unix,
/// most do, and the rest are waited for briefly. On Windows only Windows Terminal does, and
/// the console it replaced would make every program wait for answers that never come.
pub(crate) fn answers_queries() -> bool {
    !cfg!(windows) || std::env::var_os("WT_SESSION").is_some()
}

/// Single keypresses for prompts: no echo, no line buffering, Ctrl-C delivered as a byte (so
/// the terminal is always put back before leaving), and reads that give up after 0.1s so a
/// lone Esc can be told apart from the start of an arrow key. Undone when dropped.
pub(crate) struct RawMode {
    #[cfg(not(windows))]
    saved: String,
    #[cfg(windows)]
    saved: win::Saved,
}

impl RawMode {
    pub(crate) fn enter() -> io::Result<RawMode> {
        #[cfg(not(windows))]
        {
            let saved = stty(&["-g"])?;
            stty(&["-icanon", "-echo", "-isig", "min", "0", "time", "1"])?;
            Ok(RawMode { saved })
        }
        #[cfg(windows)]
        {
            Ok(RawMode {
                saved: win::enter(true)?,
            })
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        let _ = stty(&[&self.saved]);
        #[cfg(windows)]
        win::leave_raw(&self.saved);
    }
}

/// One byte from the keyboard while in [`RawMode`], or `None` if nothing came within 0.1s.
pub(crate) fn read_byte() -> io::Result<Option<u8>> {
    let mut b = [0u8; 1];
    Ok((read_some(&mut b)? == 1).then_some(b[0]))
}

/// Ask the terminal for its background colour (OSC 11). Most modern terminals answer within
/// a few milliseconds; ones that don't are given 150ms.
pub(crate) fn query_background() -> Option<(u8, u8, u8)> {
    if !interactive() || !answers_queries() {
        return None;
    }
    let cbreak = Cbreak::enter().ok()?;
    let mut out = io::stdout();
    let _ = out.write_all(b"\x1b]11;?\x1b\\");
    let _ = out.flush();
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut reply = Vec::new();
    while Instant::now() < deadline {
        let mut b = [0u8; 64];
        let n = read_some(&mut b).unwrap_or(0);
        reply.extend_from_slice(&b[..n]);
        // the reply ends with BEL or ST (ESC \)
        if reply.ends_with(b"\x07") || reply.ends_with(b"\x1b\\") {
            break;
        }
    }
    drop(cbreak);
    parse_osc_color(&String::from_utf8_lossy(&reply))
}

/// `rgb:RRRR/GGGG/BBBB` (1-4 hex digits a channel) out of an OSC colour reply.
fn parse_osc_color(reply: &str) -> Option<(u8, u8, u8)> {
    let rgb = &reply[reply.find("rgb:")? + 4..];
    let mut channels = rgb.split('/').map(|c| {
        let hex: String = c.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        let v = u32::from_str_radix(&hex, 16).ok()?;
        let max = (1u32 << (4 * hex.len() as u32)) - 1;
        Some((v * 255 / max) as u8)
    });
    Some((channels.next()??, channels.next()??, channels.next()??))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
unsafe extern "C" {
    fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
}

/// The terminal's columns and rows, if it could be asked: for [`doctor`](crate::doctor).
pub(crate) fn measured_size() -> Option<(usize, usize)> {
    ioctl_size()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn ioctl_size() -> Option<(usize, usize)> {
    #[repr(C)]
    #[allow(dead_code)]
    struct WinSize {
        rows: u16,
        cols: u16,
        xpixel: u16,
        ypixel: u16,
    }

    #[cfg(target_os = "macos")]
    const TIOCGWINSZ: std::ffi::c_ulong = 0x4008_7468;
    #[cfg(target_os = "linux")]
    const TIOCGWINSZ: std::ffi::c_ulong = 0x5413;

    let mut ws = WinSize {
        rows: 0,
        cols: 0,
        xpixel: 0,
        ypixel: 0,
    };
    // SAFETY: TIOCGWINSZ writes exactly one winsize struct through the pointer.
    let ok = unsafe { ioctl(crate::output::fd(), TIOCGWINSZ, &mut ws as *mut WinSize) } == 0;
    (ok && ws.cols > 0).then_some((ws.cols as usize, ws.rows as usize))
}

#[cfg(windows)]
fn ioctl_size() -> Option<(usize, usize)> {
    win::size()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn ioctl_size() -> Option<(usize, usize)> {
    None
}

/// How far local time is ahead of UTC at `secs` since the epoch, in seconds, from the C
/// library's idea of the time zone. 0 where that can't be found.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn utc_offset_secs(secs: i64) -> i64 {
    #[repr(C)]
    struct Tm {
        fields: [i32; 9],
        gmtoff: std::ffi::c_long,
        zone: *const std::ffi::c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let mut tm = Tm {
        fields: [0; 9],
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    // SAFETY: localtime_r fills one struct tm, whose leading fields have this layout on both
    // platforms; it returns null on failure, leaving gmtoff at 0.
    unsafe { localtime_r(&secs, &mut tm) };
    // c_long is 32 bits on some targets
    #[allow(clippy::unnecessary_cast)]
    let offset = tm.gmtoff as i64;
    offset
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn utc_offset_secs(_: i64) -> i64 {
    0
}

/// Turn on escape-code handling for output in older Windows consoles. Elsewhere, and in
/// Windows Terminal, it's always on.
pub(crate) fn enable_escapes() {
    #[cfg(windows)]
    win::enable_escapes();
}

/// The Windows console API, for what Unix does with `stty` and `ioctl`.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::io;

    type Handle = *mut c_void;

    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    const WAIT_OBJECT_0: u32 = 0;
    const KEY_EVENT: u16 = 1;
    const CTRL_BREAK_EVENT: u32 = 1;
    const UTF8: u32 = 65001;

    #[repr(C)]
    #[derive(Default)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ScreenBufferInfo {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
        max_size: Coord,
    }

    /// INPUT_RECORD, with its union read as a KEY_EVENT_RECORD (the largest member, 16 bytes).
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct InputRecord {
        kind: u16,
        _pad: u16,
        key_down: i32,
        repeat: u16,
        virtual_key: u16,
        scan_code: u16,
        ch: u16,
        control_state: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn GetConsoleMode(h: Handle, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: Handle, mode: u32) -> i32;
        fn GetConsoleCP() -> u32;
        fn SetConsoleCP(cp: u32) -> i32;
        fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
        fn PeekConsoleInputW(h: Handle, buf: *mut InputRecord, len: u32, read: *mut u32) -> i32;
        fn ReadConsoleInputW(h: Handle, buf: *mut InputRecord, len: u32, read: *mut u32) -> i32;
        fn GetConsoleScreenBufferInfo(h: Handle, info: *mut ScreenBufferInfo) -> i32;
        fn WriteConsoleInputW(h: Handle, buf: *const InputRecord, len: u32, written: *mut u32) -> i32;
        fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
        fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }

    pub(super) struct Saved {
        input: u32,
        error: u32,
        code_page: u32,
    }

    fn mode(h: Handle) -> io::Result<u32> {
        let mut m = 0;
        // SAFETY: GetConsoleMode writes one u32 through the pointer.
        if unsafe { GetConsoleMode(h, &mut m) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(m)
    }

    /// Keys a character at a time, unechoed, as escape sequences. `raw` also turns Ctrl-C
    /// into a key rather than an interruption.
    pub(super) fn enter(raw: bool) -> io::Result<Saved> {
        let mut off = ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT;
        if raw {
            off |= ENABLE_PROCESSED_INPUT;
        }
        // SAFETY: these calls only read and set console flags on standard handles.
        unsafe {
            let (input, error) = (GetStdHandle(STD_INPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE));
            let saved = Saved {
                input: mode(input)?,
                error: mode(error).unwrap_or(0),
                code_page: GetConsoleCP(),
            };
            let raw = (saved.input & !off) | ENABLE_VIRTUAL_TERMINAL_INPUT;
            if SetConsoleMode(input, raw) == 0 {
                return Err(io::Error::last_os_error());
            }
            SetConsoleMode(error, saved.error | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            SetConsoleCP(UTF8);
            Ok(saved)
        }
    }

    pub(super) fn leave_raw(saved: &Saved) {
        // SAFETY: as above, restoring values read earlier.
        unsafe {
            SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), saved.input);
            SetConsoleMode(GetStdHandle(STD_ERROR_HANDLE), saved.error);
            SetConsoleCP(saved.code_page);
        }
    }

    /// Wait up to `ms` for a keypress that will produce a character, throwing away events that
    /// won't (key releases, focus and mouse events), which would otherwise leave a read blocked.
    pub(super) fn key_waiting(ms: u32) -> bool {
        // SAFETY: the record buffer is one element long, matching the length passed.
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            if WaitForSingleObject(input, ms) != WAIT_OBJECT_0 {
                return false;
            }
            loop {
                let mut rec = InputRecord::default();
                let mut n = 0;
                if PeekConsoleInputW(input, &mut rec, 1, &mut n) == 0 || n == 0 {
                    return false;
                }
                if rec.kind == KEY_EVENT && rec.key_down != 0 && rec.ch != 0 {
                    return true;
                }
                ReadConsoleInputW(input, &mut rec, 1, &mut n);
            }
        }
    }

    pub(super) fn size() -> Option<(usize, usize)> {
        let mut info = ScreenBufferInfo::default();
        let which = if crate::output::fd() == 2 {
            STD_ERROR_HANDLE
        } else {
            STD_OUTPUT_HANDLE
        };
        // SAFETY: GetConsoleScreenBufferInfo fills one struct through the pointer.
        let ok = unsafe { GetConsoleScreenBufferInfo(GetStdHandle(which), &mut info) } != 0;
        let w = (info.right - info.left + 1) as usize;
        let h = (info.bottom - info.top + 1) as usize;
        (ok && w > 0).then_some((w, h))
    }

    /// The console's input mode, to put back after a Ctrl-C.
    pub(super) fn input_mode() -> Option<u32> {
        // SAFETY: as above.
        mode(unsafe { GetStdHandle(STD_INPUT_HANDLE) }).ok()
    }

    pub(super) fn set_input_mode(m: u32) {
        // SAFETY: as above, with a mode read earlier.
        unsafe { SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), m) };
    }

    /// Write straight to stdout (`fd` 1) or stderr (2), past std's lock on them, which a
    /// thread drawing a frame may be holding when Ctrl-C comes.
    pub(super) fn write_raw(fd: i32, bytes: &[u8]) {
        let which = if fd == 2 { STD_ERROR_HANDLE } else { STD_OUTPUT_HANDLE };
        let mut n = 0;
        // SAFETY: WriteFile reads `bytes.len()` bytes from the pointer and writes one u32.
        unsafe {
            WriteFile(
                GetStdHandle(which),
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
    }

    /// Called on a thread of its own when Ctrl-C or Ctrl-Break is pressed. Anything else
    /// (the window closing, logging off) is left to the next handler.
    pub(super) unsafe extern "system" fn on_ctrl(kind: u32) -> i32 {
        if kind > CTRL_BREAK_EVENT {
            return 0;
        }
        super::interrupted()
    }

    /// Start or stop handling Ctrl-C. Handlers stack, so each start is undone by one stop.
    pub(super) fn handle_ctrl_c(on: bool) -> bool {
        // SAFETY: on_ctrl has the signature the console calls handlers with.
        unsafe { SetConsoleCtrlHandler(Some(on_ctrl), i32::from(on)) != 0 }
    }

    /// Type `text` into the console's input, as key presses. Escape sequences are left out:
    /// the shell reads keys, not sequences, and would take their Esc as a press of Esc.
    pub(super) fn push_back(text: &str) {
        let text = crate::text::strip_ansi(text);
        let records: Vec<InputRecord> = text
            .chars()
            .filter(|&c| c == '\r' || !c.is_control())
            .collect::<String>()
            .encode_utf16()
            .map(|ch| InputRecord {
                kind: KEY_EVENT,
                key_down: 1,
                repeat: 1,
                ch,
                ..InputRecord::default()
            })
            .collect();
        if records.is_empty() {
            return;
        }
        let mut n = 0;
        // SAFETY: WriteConsoleInputW reads `records.len()` records and writes one u32.
        unsafe {
            WriteConsoleInputW(
                GetStdHandle(STD_INPUT_HANDLE),
                records.as_ptr(),
                records.len() as u32,
                &mut n,
            )
        };
    }

    pub(super) fn enable_escapes() {
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: as above.
            unsafe {
                let h = GetStdHandle(which);
                if let Ok(m) = mode(h) {
                    SetConsoleMode(h, m | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

/// Put bytes back into the terminal's input queue, so keys typed while tinsel was reading
/// stdin reach the shell afterwards. Best effort: newer Linux kernels refuse (TIOCSTI).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn push_back_input(bytes: &[u8]) {
    push_back_to(0, bytes);
}

/// [`push_back_input`], to the terminal open as `fd`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn push_back_to(fd: i32, bytes: &[u8]) {
    #[cfg(target_os = "macos")]
    const TIOCSTI: std::ffi::c_ulong = 0x8001_7472;
    #[cfg(target_os = "linux")]
    const TIOCSTI: std::ffi::c_ulong = 0x5412;
    for b in bytes {
        // SAFETY: TIOCSTI reads one byte through the pointer.
        if unsafe { ioctl(fd, TIOCSTI, b as *const u8) } != 0 {
            return;
        }
    }
}

#[cfg(windows)]
pub(crate) fn push_back_input(bytes: &[u8]) {
    win::push_back(&String::from_utf8_lossy(bytes));
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) fn push_back_input(_: &[u8]) {}

// MARK: - Ctrl-C

/// What Ctrl-C does while tinsel draws something live: a spinner, progress bar, steps, a
/// [`Region`](crate::Region) or the help pager. Set with [`set_ctrl_c`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtrlC {
    /// Put the terminal back and exit with code 130. The default.
    Exit,
    /// The same, printing this line on the way out, so the program can say what was stopped.
    ExitWith(String),
    /// Leave Ctrl-C to the program's own handler, so it can stop its work its own way. The
    /// terminal is put back when the drawing ends, as usual.
    Leave,
}

static LEAVE_CTRL_C: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The line to print on the way out, leaked so the signal handler only ever reads it.
static CTRL_C_LINE: std::sync::atomic::AtomicPtr<Vec<u8>> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Choose what Ctrl-C does while tinsel draws something live. Call it early in `main`.
///
/// ```
/// tinsel::set_ctrl_c(tinsel::CtrlC::ExitWith("stopped: nothing was changed".into()));
/// ```
pub fn set_ctrl_c(how: CtrlC) {
    use std::sync::atomic::Ordering::SeqCst;
    LEAVE_CTRL_C.store(how == CtrlC::Leave, SeqCst);
    let line = match how {
        CtrlC::ExitWith(line) => Box::into_raw(Box::new(format!("{line}\n").into_bytes())),
        CtrlC::Exit | CtrlC::Leave => std::ptr::null_mut(),
    };
    // the old line is left alone: the handler may be reading it right now
    CTRL_C_LINE.store(line, SeqCst);
}

/// While tinsel has the cursor hidden or the terminal in a special mode, Ctrl-C would leave
/// the user's shell that way. Returns the previous handler so it can be put back afterwards.
#[cfg(unix)]
pub(crate) fn guard_interrupt() -> Option<usize> {
    if LEAVE_CTRL_C.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    // SAFETY: on_interrupt only calls async-signal-safe functions.
    let prev = unsafe { signal(SIGINT, on_interrupt as extern "C" fn(i32) as usize) };
    (prev != SIG_ERR).then_some(prev)
}

#[cfg(unix)]
pub(crate) fn restore_interrupt(prev: Option<usize>) {
    if let Some(prev) = prev {
        // SAFETY: prev came from signal() itself, so it is a valid handler value.
        unsafe { signal(SIGINT, prev) };
    }
}

/// Snapshot the terminal's settings so the Ctrl-C handler can restore them. The snapshot is
/// kept as opaque bytes: tcsetattr is safe to call from a signal handler, `stty` isn't, and
/// handing back exactly what tcgetattr gave means never needing to know the struct's layout.
#[cfg(unix)]
pub(crate) fn remember_terminal() {
    // SAFETY: the buffer is larger than any platform's termios struct.
    let ok = unsafe { tcgetattr(0, (&raw mut SAVED_TERMIOS).cast()) } == 0;
    HAVE_SAVED.store(ok, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn forget_terminal() {
    HAVE_SAVED.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Put back the settings [`remember_terminal`] saved, if any, and stop focus reports.
#[cfg(unix)]
pub(crate) fn restore_saved_terminal() {
    if HAVE_SAVED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        // SAFETY: SAVED_TERMIOS holds exactly what tcgetattr wrote.
        unsafe { tcsetattr(0, TCSANOW, (&raw const SAVED_TERMIOS).cast()) };
    }
    crate::output::write(std::str::from_utf8(RESTORE).unwrap_or_default());
}

#[cfg(unix)]
const SIGINT: i32 = 2;
#[cfg(unix)]
const SIG_ERR: usize = usize::MAX;
#[cfg(unix)]
const TCSANOW: i32 = 0;

#[cfg(unix)]
static mut SAVED_TERMIOS: [u8; 256] = [0; 256];
static HAVE_SAVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The console's input mode, saved for Ctrl-C to put back.
#[cfg(windows)]
static SAVED_MODE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// What Ctrl-C writes on its way out: focus reports off, the cursor back, line wrapping on.
const RESTORE: &[u8] = b"\x1b[?1004l\x1b[?25h\x1b[?7h";

#[cfg(unix)]
unsafe extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
    fn write(fd: i32, buf: *const std::ffi::c_void, count: usize) -> isize;
    fn _exit(code: i32) -> !;
    fn tcgetattr(fd: i32, termios: *mut u8) -> i32;
    fn tcsetattr(fd: i32, action: i32, termios: *const u8) -> i32;
}

#[cfg(unix)]
extern "C" fn on_interrupt(_: i32) {
    // SAFETY: write, tcsetattr and _exit are all async-signal-safe, and SAVED_TERMIOS is only
    // written before HAVE_SAVED is set.
    unsafe {
        if HAVE_SAVED.load(std::sync::atomic::Ordering::SeqCst) {
            tcsetattr(0, TCSANOW, (&raw const SAVED_TERMIOS).cast());
        }
        let fd = crate::output::fd_now();
        write(fd, RESTORE.as_ptr().cast(), RESTORE.len());
        write(fd, b"\n".as_ptr().cast(), 1);
        let line = CTRL_C_LINE.load(std::sync::atomic::Ordering::SeqCst);
        if !line.is_null() {
            write(fd, (*line).as_ptr().cast(), (*line).len());
        }
        _exit(130)
    }
}

/// On Windows Ctrl-C is a call to a handler on a thread of its own, not a signal, so the
/// handler can do as it likes. Returns something only so [`restore_interrupt`] knows to undo it.
#[cfg(windows)]
pub(crate) fn guard_interrupt() -> Option<usize> {
    if LEAVE_CTRL_C.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    win::handle_ctrl_c(true).then_some(0)
}

#[cfg(windows)]
pub(crate) fn restore_interrupt(prev: Option<usize>) {
    if prev.is_some() {
        win::handle_ctrl_c(false);
    }
}

#[cfg(windows)]
pub(crate) fn remember_terminal() {
    if let Some(mode) = win::input_mode() {
        SAVED_MODE.store(mode, std::sync::atomic::Ordering::SeqCst);
        HAVE_SAVED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(windows)]
pub(crate) fn restore_saved_terminal() {
    if HAVE_SAVED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        win::set_input_mode(SAVED_MODE.load(std::sync::atomic::Ordering::SeqCst));
    }
    crate::output::write(std::str::from_utf8(RESTORE).unwrap_or_default());
}

/// Ctrl-C on Windows: the console put back as it was, the cursor shown, the program's line
/// said, and out with 130, as on Unix.
#[cfg(windows)]
fn interrupted() -> ! {
    use std::sync::atomic::Ordering::SeqCst;
    if HAVE_SAVED.swap(false, SeqCst) {
        win::set_input_mode(SAVED_MODE.load(SeqCst));
    }
    let fd = crate::output::fd_now();
    win::write_raw(fd, RESTORE);
    win::write_raw(fd, b"\n");
    let line = CTRL_C_LINE.load(SeqCst);
    if !line.is_null() {
        // SAFETY: lines are leaked when set and never freed, so this one is still there
        win::write_raw(fd, unsafe { &*line });
    }
    std::process::exit(130)
}

/// Ctrl-C, pressed by a test: a signal on Unix, and on Windows a call to the handler as the
/// console would make it (asking the console means interrupting the test runner too).
#[cfg(test)]
pub(crate) fn press_ctrl_c() {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn raise(sig: i32) -> i32;
        }
        // SAFETY: raise only delivers a signal, to whichever handler is set
        unsafe { raise(SIGINT) };
    }
    #[cfg(windows)]
    // SAFETY: as the console calls it, with Ctrl-C
    unsafe {
        win::on_ctrl(0);
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn guard_interrupt() -> Option<usize> {
    None
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn restore_interrupt(_: Option<usize>) {}

#[cfg(not(any(unix, windows)))]
pub(crate) fn remember_terminal() {}

#[cfg(not(any(unix, windows)))]
pub(crate) fn restore_saved_terminal() {
    crate::output::write(std::str::from_utf8(RESTORE).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_osc_colour_replies() {
        assert_eq!(parse_osc_color("\x1b]11;rgb:ffff/ffff/ffff\x07"), Some((255, 255, 255)));
        assert_eq!(parse_osc_color("\x1b]11;rgb:1e1e/2020/2a2a\x1b\\"), Some((30, 32, 42)));
        assert_eq!(parse_osc_color("\x1b]11;rgb:ff/80/00\x07"), Some((255, 128, 0)));
        assert_eq!(parse_osc_color("garbage"), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn stty_says_when_it_couldnt() {
        assert!(stty(&["--no-such-setting"]).is_err());
    }

    #[test]
    fn nothing_is_asked_with_no_one_there() {
        let _ = interactive();
        assert_eq!(crate::output::with_terminal(false, query_background), None);
    }

    #[test]
    fn the_terminal_is_put_back_as_it_was() {
        // remembered first, so on a real terminal it's put back exactly as it is
        remember_terminal();
        HAVE_SAVED.store(true, std::sync::atomic::Ordering::SeqCst);
        let out = crate::output::capture(restore_saved_terminal);
        assert!(out.contains("\x1b[?25h"));
        assert!(!HAVE_SAVED.load(std::sync::atomic::Ordering::SeqCst));
        // nothing saved now, so only the cursor and the rest come back
        assert!(crate::output::capture(restore_saved_terminal).contains("\x1b[?25h"));
        forget_terminal();
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn typed_keys_go_back_only_to_a_terminal() {
        use std::os::fd::AsRawFd;
        // a file isn't a terminal, so there's nowhere to put them
        let file = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        push_back_to(file.as_raw_fd(), b"x");
    }

    #[test]
    fn sizes_fall_back_to_the_environment_then_a_default() {
        assert_eq!(measured_or(Some(120), "TINSEL_NO_SUCH_VARIABLE", 80), 120);
        assert_eq!(measured_or(None, "TINSEL_NO_SUCH_VARIABLE", 24), 24);
        assert_eq!(measured_or(Some(0), "TINSEL_NO_SUCH_VARIABLE", 80), 80);
    }
}

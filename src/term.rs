//! The terminal itself: its size, its modes, its background colour, and putting everything
//! back on Ctrl-C. Calls libc directly and shells out to `stty`, to stay dependency-free.

use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The width tinsel lays things out in: the terminal's, capped so lines stay readable.
pub fn content_width() -> usize {
    width().min(76)
}

/// Columns in the terminal attached to stdout, falling back to `$COLUMNS`, then 80.
pub fn width() -> usize {
    ioctl_size()
        .map(|(cols, _)| cols)
        .or_else(|| std::env::var("COLUMNS").ok()?.parse().ok())
        .filter(|&w| w > 0)
        .unwrap_or(80)
}

/// Rows in the terminal attached to stdout, falling back to `$LINES`, then 24.
pub(crate) fn height() -> usize {
    ioctl_size()
        .map(|(_, rows)| rows)
        .or_else(|| std::env::var("LINES").ok()?.parse().ok())
        .filter(|&h| h > 0)
        .unwrap_or(24)
}

/// Whether both ends are a terminal: something to draw on, and someone typing.
pub(crate) fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Run `stty` against the terminal on stdin. `stty` is used rather than termios structs
/// because their layout differs between operating systems.
pub(crate) fn stty(args: &[&str]) -> io::Result<String> {
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

/// Character-at-a-time input with no echo, reads timing out every 0.1s. Returns the
/// settings to hand back to [`stty`] afterwards.
pub(crate) fn enter_cbreak() -> io::Result<String> {
    let saved = stty(&["-g"])?;
    stty(&["-icanon", "-echo", "min", "0", "time", "1"])?;
    Ok(saved)
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
                saved: win::enter_raw()?,
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
    #[cfg(windows)]
    if !win::key_waiting(100) {
        return Ok(None);
    }
    let mut b = [0u8; 1];
    Ok((io::stdin().lock().read(&mut b)? == 1).then_some(b[0]))
}

/// Ask the terminal for its background colour (OSC 11). Most modern terminals answer within
/// a few milliseconds; ones that don't are given 150ms.
pub(crate) fn query_background() -> Option<(u8, u8, u8)> {
    if !interactive() {
        return None;
    }
    let saved = enter_cbreak().ok()?;
    let mut out = io::stdout();
    let _ = out.write_all(b"\x1b]11;?\x1b\\");
    let _ = out.flush();
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut reply = Vec::new();
    let mut stdin = io::stdin().lock();
    while Instant::now() < deadline {
        let mut b = [0u8; 64];
        let n = stdin.read(&mut b).unwrap_or(0);
        reply.extend_from_slice(&b[..n]);
        // the reply ends with BEL or ST (ESC \)
        if reply.ends_with(b"\x07") || reply.ends_with(b"\x1b\\") {
            break;
        }
    }
    let _ = stty(&[&saved]);
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
    let ok = unsafe { ioctl(1, TIOCGWINSZ, &mut ws as *mut WinSize) } == 0;
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

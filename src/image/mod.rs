//! Pictures shown at the terminal's full resolution, through kitty's graphics protocol or
//! iTerm2's inline images. A terminal that speaks neither can't show them at all: there's
//! no blocky stand-in, so an app asks [`support`] first and says so instead.
//!
//! An app puts a picture in an area of its frame with [`Frame::image`]; the run loop sends
//! it, fitted to the area and keeping its shape, and takes it away once a frame no longer
//! has it. A picture sits above the text, so nothing else should be drawn where it goes.
//!
//! iTerm2 reads PNG, JPEG and GIF files itself. kitty's protocol takes PNG files as they
//! are, and JPEG and GIF as pixels, which the `decode` feature decodes in tinsel, shrunk to
//! no bigger than the screen.
//!
//! [`Frame::image`]: crate::app::Frame::image

use std::cell::Cell;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(feature = "decode")]
use std::sync::{Mutex, PoisonError};

use crate::app::Rect;

#[cfg(feature = "decode")]
mod decode;

/// An image file's bytes, with its format and size read from its header. The terminal
/// decodes it, except where kitty is sent a JPEG or GIF, which the `decode` feature turns
/// into pixels first.
#[derive(Clone)]
pub struct Image {
    // shared with the thread that decodes it, for a big one
    bytes: Arc<[u8]>,
    format: Format,
    // as it's meant to be seen, turned as its EXIF orientation says
    width: u32,
    height: u32,
    // EXIF's 1 to 8: as it is, or which way it's turned or mirrored
    #[cfg_attr(not(feature = "decode"), allow(dead_code))]
    orientation: u8,
    // the same for every frame it's put in, so the terminal is sent it once
    id: u32,
    // decoded the first time kitty needs it, and kept; a big one on a thread of its own
    #[cfg(feature = "decode")]
    pixels: Arc<OnceLock<Result<decode::Pixels, String>>>,
    #[cfg(feature = "decode")]
    decoding: Arc<AtomicBool>,
    // the pixels as last sent to kitty, shrunk to a screen of this size, so sending it again
    // after the screen's been left costs nothing
    #[cfg(feature = "decode")]
    fitted: Arc<Mutex<Option<Fitted>>>,
}

// base64 for kitty, and the keys saying what it is
type Payload = (String, String);

// a payload, and the screen it was shrunk to
#[cfg(feature = "decode")]
type Fitted = ((u32, u32), Payload);

// one image, however many times it's cloned
impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Image {}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Image({} {}x{}, {} bytes)",
            self.format.name(),
            self.width,
            self.height,
            self.bytes.len()
        )
    }
}

/// The kinds of image file [`Image::new`] knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// Portable Network Graphics.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF, of which terminals show the first frame.
    Gif,
}

impl Format {
    /// Its usual name, as in PNG.
    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Gif => "GIF",
        }
    }
}

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

// files this big are decoded on a thread of their own: a few milliseconds is fine to wait
// for in a frame, but a big photo takes longer
#[cfg(feature = "decode")]
const DECODE_HERE: usize = 1 << 20;

// held by a test to keep a big picture's decode from finishing while it looks
#[cfg(all(test, feature = "decode"))]
pub(crate) static HOLD: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Image {
    /// The image in `bytes`, or why it isn't one tinsel knows.
    ///
    /// # Errors
    ///
    /// Why `bytes` aren't a PNG, JPEG or GIF tinsel can read the size of, in words.
    ///
    /// ```
    /// use tinsel::image::{Format, Image};
    ///
    /// let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    /// png.extend([0, 0, 0, 3, 0, 0, 0, 2]);
    /// let image = Image::new(png).unwrap();
    /// assert_eq!((image.format(), image.width(), image.height()), (Format::Png, 3, 2));
    /// assert!(Image::new(b"hello".to_vec()).is_err());
    /// ```
    pub fn new(bytes: Vec<u8>) -> Result<Image, String> {
        let (format, width, height, orientation) =
            sniff(&bytes).ok_or("not an image tinsel knows: PNG, JPEG or GIF")?;
        if width == 0 || height == 0 {
            return Err(format!("a {} with no pixels", format.name()));
        }
        // turned a quarter, it's as wide as it was tall
        let (width, height) = if orientation >= 5 {
            (height, width)
        } else {
            (width, height)
        };
        Ok(Image {
            bytes: bytes.into(),
            format,
            width,
            height,
            orientation,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            #[cfg(feature = "decode")]
            pixels: Arc::default(),
            #[cfg(feature = "decode")]
            decoding: Arc::default(),
            #[cfg(feature = "decode")]
            fitted: Arc::default(),
        })
    }

    /// Its pixels, decoded once and kept, or why they can't be had; waits for a decode
    /// already going on another thread.
    #[cfg(feature = "decode")]
    pub(crate) fn pixels(&self) -> Result<&decode::Pixels, &str> {
        self.pixels
            .get_or_init(|| decode::decode(&self.bytes, self.format, self.orientation))
            .as_ref()
            .map_err(String::as_str)
    }

    /// Starts decoding it for kitty, if it needs it: a small file straight away, and a big
    /// one on another thread, so the app can draw something meanwhile.
    #[cfg(feature = "decode")]
    fn start_decoding(&self) {
        if self.format == Format::Png || self.pixels.get().is_some() {
            return;
        }
        if self.bytes.len() < DECODE_HERE {
            let _ = self.pixels();
        } else if !self.decoding.swap(true, Ordering::Relaxed) {
            let image = self.clone();
            std::thread::spawn(move || {
                #[cfg(test)]
                let _held = HOLD.lock();
                // shrunk here too, for the screen as it is, so the run loop needn't
                if image.pixels().is_ok() {
                    let cell = crate::term::cell_pixels().unwrap_or((8, 16));
                    let screen = (
                        crate::term::width() as u32 * cell.0,
                        crate::term::height() as u32 * cell.1,
                    );
                    let _ = kitty_data(&image, screen);
                }
            });
        }
    }

    /// Whether `protocol` can be sent it now: not while it's still decoding for kitty, nor
    /// once that's failed, when kitty would have nothing to show. Starts the decode if it's
    /// yet to start, for a picture put in a frame without a [`Support::check`] first.
    pub(crate) fn ready_for(&self, protocol: Protocol) -> bool {
        #[cfg(feature = "decode")]
        if protocol == Protocol::Kitty && self.format != Format::Png {
            self.start_decoding();
            return matches!(self.pixels.get(), Some(Ok(_)));
        }
        protocol.shows(self.format)
    }

    /// Whether it's still being decoded on another thread, as a big JPEG or GIF is the first
    /// time kitty is to show it. Until it's done the run loop holds it back, so an app can
    /// show something else there meanwhile; something that moves, so the next frame comes.
    pub fn pending(&self) -> bool {
        #[cfg(feature = "decode")]
        {
            self.decoding.load(Ordering::Relaxed) && self.pixels.get().is_none()
        }
        #[cfg(not(feature = "decode"))]
        false
    }

    /// Whether `bytes` start the way an image tinsel knows does, as a cheap test before
    /// reading all of a file.
    pub fn looks_like(bytes: &[u8]) -> bool {
        bytes.starts_with(PNG) || bytes.starts_with(b"\xff\xd8\xff") || bytes.starts_with(b"GIF8")
    }

    /// What kind of file it is.
    pub fn format(&self) -> Format {
        self.format
    }

    /// How wide it is, in pixels, turned the way it's meant to be seen.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// How tall it is, in pixels, turned the way it's meant to be seen.
    pub fn height(&self) -> u32 {
        self.height
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

// the format, size and EXIF orientation in an image file's header
fn sniff(b: &[u8]) -> Option<(Format, u32, u32, u8)> {
    let be32 = |i: usize| Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?));
    let le16 = |i: usize| Some(u32::from(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?)));
    if b.starts_with(PNG) && b.get(12..16) == Some(b"IHDR") {
        return Some((Format::Png, be32(16)?, be32(20)?, 1));
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some((Format::Gif, le16(6)?, le16(8)?, 1));
    }
    if b.starts_with(b"\xff\xd8") {
        // segments one after another, until the frame header that holds the size, with the
        // orientation in the EXIF segment before it, if there is one
        let mut segments = Segments { b, at: 2 };
        let mut orientation = 1;
        loop {
            // the size is enough, from a file cut short after it
            let (marker, seg) = segments.next()?;
            let seg = seg.unwrap_or_else(|part| part);
            let frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
            if frame {
                let be16 = |i: usize| Some(u32::from(u16::from_be_bytes(seg.get(i..i + 2)?.try_into().ok()?)));
                return Some((Format::Jpeg, be16(3)?, be16(1)?, orientation));
            }
            match marker {
                0xe1 => orientation = exif_orientation(seg).unwrap_or(orientation),
                0xd9 => return None,
                _ => {}
            }
        }
    }
    None
}

// a segment's bytes, or as an error the part there is of one cut short
type Segment<'a> = Result<&'a [u8], &'a [u8]>;

/// A JPEG's segments one after another, from `at`, which the decoder moves past a scan's
/// data itself.
pub(crate) struct Segments<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl<'a> Segments<'a> {
    /// The next marker and its segment's bytes, after its length: `None` where there's no
    /// marker, and as an error the part there is where the segment is cut short. Markers
    /// that stand alone, with no segment, are passed over, except the end, which comes with none.
    pub(crate) fn next(&mut self) -> Option<(u8, Segment<'a>)> {
        let b = self.b;
        let marker = loop {
            // markers may be padded with any number of 0xff
            while b.get(self.at) == Some(&0xff) && b.get(self.at + 1) == Some(&0xff) {
                self.at += 1;
            }
            let (Some(&0xff), Some(&marker)) = (b.get(self.at), b.get(self.at + 1)) else {
                return None;
            };
            self.at += 2;
            match marker {
                0xd8 | 0x01 | 0xd0..=0xd7 => continue,
                0xd9 => return Some((marker, Ok(&[]))),
                _ => break marker,
            }
        };
        let len = match b.get(self.at..self.at + 2) {
            Some(l) => usize::from(u16::from_be_bytes([l[0], l[1]])),
            None => usize::MAX,
        };
        let seg = b
            .get(self.at + 2..self.at.saturating_add(len))
            .ok_or_else(|| b.get(self.at + 2..).unwrap_or(&[]));
        self.at = self.at.saturating_add(len).min(b.len());
        Some((marker, seg))
    }
}

/// The orientation in an APP1 segment's EXIF, if it has one: 1 to 8, as EXIF numbers them.
fn exif_orientation(seg: &[u8]) -> Option<u8> {
    let tiff = seg.strip_prefix(b"Exif\0\0")?;
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |i: usize| {
        let b: [u8; 2] = tiff.get(i..i + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |i: usize| {
        let b: [u8; 4] = tiff.get(i..i + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    // the first directory's entries, twelve bytes each: tag, type, count, value
    (0..usize::from(u16_at(ifd)?))
        .map(|i| ifd + 2 + i * 12)
        .find(|&e| u16_at(e) == Some(0x0112))
        .and_then(|e| u16_at(e + 8))
        .and_then(|o| u8::try_from(o).ok())
        .filter(|o| (1..=8).contains(o))
}

/// How the terminal shows pictures, if it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Protocol {
    /// kitty's graphics protocol: kitty, Ghostty and Konsole. It takes PNG as it is, and
    /// JPEG and GIF as pixels with the `decode` feature.
    Kitty,
    /// iTerm2's inline images: iTerm2 and WezTerm. The terminal reads PNG, JPEG and GIF.
    Iterm,
}

impl Protocol {
    /// Whether this protocol can show `format` without tinsel decoding it.
    pub fn shows(self, format: Format) -> bool {
        match self {
            Protocol::Kitty => format == Format::Png || cfg!(feature = "decode"),
            Protocol::Iterm => true,
        }
    }
}

/// Whether, and how, this terminal can show pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Support {
    /// It can, this way.
    Yes(Protocol),
    /// It can't: no protocol tinsel knows, or a multiplexer like tmux in the way.
    No,
}

impl Support {
    /// Whether it can show `image`, or why not, in words for the app to show instead.
    ///
    /// # Errors
    ///
    /// Why not: the terminal shows no pictures, or not ones in this format.
    ///
    /// ```
    /// use tinsel::image::{Protocol, Support};
    ///
    /// assert!(Support::No.check_format(tinsel::image::Format::Png).unwrap_err().contains("can't show"));
    /// assert!(Support::Yes(Protocol::Iterm).check_format(tinsel::image::Format::Jpeg).is_ok());
    /// ```
    pub fn check(self, image: &Image) -> Result<Protocol, String> {
        let protocol = self.check_format(image.format)?;
        // kitty needs the pixels, so a picture that won't decode can't be shown; a big one is
        // only known not to once it's been tried, so it's fine until then
        #[cfg(feature = "decode")]
        if protocol == Protocol::Kitty && image.format != Format::Png {
            image.start_decoding();
            if let Some(Err(why)) = image.pixels.get() {
                return Err(format!("can't read it: {why}"));
            }
        }
        Ok(protocol)
    }

    /// [`check`](Support::check), for a format.
    ///
    /// # Errors
    ///
    /// As [`check`](Support::check)'s.
    pub fn check_format(self, format: Format) -> Result<Protocol, String> {
        match self {
            Support::No => {
                Err("this terminal can't show images: kitty, Ghostty, Konsole, iTerm2 and WezTerm can".into())
            }
            Support::Yes(p) if p.shows(format) => Ok(p),
            Support::Yes(_) => Err(format!(
                "this terminal shows PNG images, and this is a {}",
                format.name()
            )),
        }
    }
}

thread_local! {
    static FORCED: Cell<Option<Support>> = const { Cell::new(None) };
}

/// Whether, and how, the terminal tinsel runs in can show pictures, going by its environment.
/// `TINSEL_IMAGES=kitty`, `iterm` or `none` decides it instead.
pub fn support() -> Support {
    if let Some(s) = FORCED.with(Cell::get) {
        return s;
    }
    static FOUND: OnceLock<Support> = OnceLock::new();
    *FOUND.get_or_init(|| support_in(|k| std::env::var(k).ok().filter(|v| !v.is_empty())))
}

/// Runs `f` as if the terminal's support were `s`, for tests.
pub fn with_support<T>(s: Support, f: impl FnOnce() -> T) -> T {
    let before = FORCED.with(|c| c.replace(Some(s)));
    let out = f();
    FORCED.with(|c| c.set(before));
    out
}

fn support_in(var: impl Fn(&str) -> Option<String>) -> Support {
    match var("TINSEL_IMAGES").as_deref() {
        Some("kitty") => return Support::Yes(Protocol::Kitty),
        Some("iterm") => return Support::Yes(Protocol::Iterm),
        Some("none") => return Support::No,
        _ => {}
    }
    // tmux and screen hold on to the escapes unless set up to pass them on
    if var("TMUX").is_some() || var("TERM").is_some_and(|t| t.starts_with("screen") || t.starts_with("tmux")) {
        return Support::No;
    }
    let program = var("TERM_PROGRAM").unwrap_or_default();
    let term = var("TERM").unwrap_or_default();
    match program.as_str() {
        "iTerm.app" | "WezTerm" => Support::Yes(Protocol::Iterm),
        "ghostty" => Support::Yes(Protocol::Kitty),
        _ if term.contains("kitty") || term.contains("ghostty") || var("KITTY_WINDOW_ID").is_some() => {
            Support::Yes(Protocol::Kitty)
        }
        _ if var("KONSOLE_VERSION").is_some() => Support::Yes(Protocol::Kitty),
        _ => Support::No,
    }
}

/// Where in `area` an image `width` by `height` pixels goes, given a cell's size in pixels:
/// as big as fits without stretching it or making it bigger than it is, in the middle.
pub(crate) fn fit((width, height): (u32, u32), (cell_w, cell_h): (u32, u32), area: Rect) -> Rect {
    let (cell_w, cell_h) = (f64::from(cell_w.max(1)), f64::from(cell_h.max(1)));
    let room = (area.width as f64 * cell_w, area.height as f64 * cell_h);
    let scale = (room.0 / f64::from(width)).min(room.1 / f64::from(height)).min(1.0);
    let cols = ((f64::from(width) * scale / cell_w).round() as usize).clamp(1, area.width.max(1));
    let rows = ((f64::from(height) * scale / cell_h).round() as usize).clamp(1, area.height.max(1));
    Rect::new(
        area.x + (area.width.saturating_sub(cols)) / 2,
        area.y + (area.height.saturating_sub(rows)) / 2,
        cols,
        rows,
    )
}

/// What the run loop knows of the pictures on screen, to send only what changed.
#[derive(Default)]
pub(crate) struct Shown {
    placed: Vec<(Rect, u32)>,
    // images kitty already has, by id
    sent: Vec<u32>,
    left: u32,
}

// whether a picture has been sent since the screen was last left, so leaving clears them
static ANY_SENT: AtomicBool = AtomicBool::new(false);
// how many times the screen's been left with pictures on it, after which kitty has none
static LEFT: AtomicU32 = AtomicU32::new(0);

impl Shown {
    /// What to send before a frame: nothing, unless the screen was drawn afresh (`redrawn`),
    /// when kitty's pictures are taken off it to go back where the frame wants them. After
    /// the app left the screen, kitty has none left to take off, or to place again unsent.
    pub(crate) fn before_frame(&mut self, redrawn: bool, protocol: Option<Protocol>) -> &'static str {
        let left = LEFT.load(Ordering::Relaxed);
        if left != self.left {
            self.left = left;
            self.placed.clear();
            self.sent.clear();
            return "";
        }
        if !redrawn || self.placed.is_empty() {
            return "";
        }
        self.placed.clear();
        if protocol == Some(Protocol::Kitty) {
            "\x1b_Ga=d,d=a,q=2\x1b\\"
        } else {
            ""
        }
    }

    /// Whether no pictures are on screen, nor kept in the terminal to free.
    pub(crate) fn is_empty(&self) -> bool {
        self.placed.is_empty() && self.sent.is_empty()
    }

    /// Whether `wanted` puts pictures anywhere other than where they are now.
    pub(crate) fn changes(&self, wanted: &[(Rect, Image)]) -> bool {
        let now: Vec<(Rect, u32)> = wanted.iter().map(|(r, i)| (*r, i.id)).collect();
        now != self.placed
    }

    /// The escapes that take the pictures on screen away and put `wanted` there instead,
    /// each already fitted to its area. For iTerm2, whose pictures are part of the cells,
    /// the cells under the old ones have to be drawn again first.
    pub(crate) fn update(&mut self, wanted: &[(Rect, Image)], protocol: Protocol, screen: (u32, u32)) -> String {
        let mut out = String::new();
        if protocol == Protocol::Kitty {
            let kept = |id: u32| wanted.iter().any(|(_, i)| i.id == id);
            for (_, id) in self.placed.iter().filter(|(_, id)| kept(*id)) {
                // lowercase: the placement goes, and kitty keeps the image to place again
                let _ = write!(out, "\x1b_Ga=d,d=i,i={id},q=2\x1b\\");
            }
            for id in self.sent.iter().filter(|id| !kept(**id)) {
                // uppercase: the image goes too, as nothing shows it, rather than fill kitty's
                // storage till it drops ones tinsel thinks it still has
                let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
            }
            self.sent.retain(|id| kept(*id));
        }
        for (at, image) in wanted {
            if protocol == Protocol::Kitty {
                ANY_SENT.store(true, Ordering::Relaxed);
            }
            if protocol == Protocol::Kitty && !self.sent.contains(&image.id) {
                kitty_send(&mut out, image, screen);
                self.sent.push(image.id);
            }
            let _ = write!(out, "\x1b[{};{}H", at.y + 1, at.x + 1);
            match protocol {
                Protocol::Kitty => {
                    let _ = write!(
                        out,
                        "\x1b_Ga=p,i={},c={},r={},C=1,q=2\x1b\\",
                        image.id, at.width, at.height
                    );
                }
                Protocol::Iterm => {
                    let _ = write!(
                        out,
                        "\x1b]1337;File=inline=1;size={};width={};height={};preserveAspectRatio=1:{}\x07",
                        image.bytes.len(),
                        at.width,
                        at.height,
                        crate::clipboard::base64(&image.bytes)
                    );
                }
            }
        }
        self.placed = wanted.iter().map(|(r, i)| (*r, i.id)).collect();
        out
    }
}

/// The image to kitty in pieces of at most 4096 base64 characters, as the protocol asks: a
/// PNG as it is, and anything else as its pixels, made no bigger than the `screen` (in pixels)
/// it could fill, as more would only be shrunk again.
fn kitty_send(out: &mut String, image: &Image, screen: (u32, u32)) {
    let (data, keys) = kitty_data(image, screen);
    let pieces: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
    for (i, piece) in pieces.iter().enumerate() {
        let more = u8::from(i + 1 < pieces.len());
        let piece = std::str::from_utf8(piece).unwrap_or("");
        if i == 0 {
            let _ = write!(out, "\x1b_Ga=t,{keys},t=d,i={},q=2,m={more};{piece}\x1b\\", image.id);
        } else {
            let _ = write!(out, "\x1b_Gm={more};{piece}\x1b\\");
        }
    }
}

// the base64 to send kitty, and the keys saying what it is
fn kitty_data(image: &Image, screen: (u32, u32)) -> Payload {
    #[cfg(feature = "decode")]
    if image.format != Format::Png {
        if let Ok(p) = image.pixels() {
            let mut fitted = image.fitted.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((_, payload)) = fitted.as_ref().filter(|(at, _)| *at == screen) {
                return payload.clone();
            }
            let p = p.shrunk_to(screen.0.max(1), screen.1.max(1));
            let (bytes, f) = if p.opaque() {
                (p.rgba.chunks_exact(4).flat_map(|c| &c[..3]).copied().collect(), 24)
            } else {
                (p.rgba, 32)
            };
            let payload = (
                crate::clipboard::base64(&bytes),
                format!("f={f},s={},v={}", p.width, p.height),
            );
            *fitted = Some((screen, payload.clone()));
            return payload;
        }
    }
    let _ = screen;
    (crate::clipboard::base64(&image.bytes), "f=100".into())
}

/// What to send as the app leaves the screen: kitty told to drop every picture, if any were
/// sent, so none are left behind in the terminal. iTerm2's go with the cells they're in.
pub(crate) fn leaving() -> &'static str {
    if ANY_SENT.swap(false, Ordering::Relaxed) {
        LEFT.fetch_add(1, Ordering::Relaxed);
        "\x1b_Ga=d,d=A,q=2\x1b\\"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = PNG.to_vec();
        b.extend(13u32.to_be_bytes());
        b.extend(b"IHDR");
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b
    }

    #[test]
    fn formats_and_sizes_come_from_the_header() {
        assert_eq!(sniff(&png(640, 480)), Some((Format::Png, 640, 480, 1)));
        assert_eq!(sniff(b"GIF89a\x40\x01\xf0\x00"), Some((Format::Gif, 320, 240, 1)));
        // a JPEG with an APP0 segment before its frame header
        let mut jpeg = b"\xff\xd8\xff\xe0\x00\x04ab".to_vec();
        jpeg.extend(b"\xff\xc0\x00\x11\x08\x01\xe0\x02\x80");
        jpeg.extend([0; 10]);
        assert_eq!(sniff(&jpeg), Some((Format::Jpeg, 640, 480, 1)));
        // a restart marker standing alone before the frame header, as the decoder allows
        let mut stray = jpeg[..8].to_vec();
        stray.extend(b"\xff\xd3");
        stray.extend(&jpeg[8..]);
        assert_eq!(sniff(&stray), Some((Format::Jpeg, 640, 480, 1)));
        // and millions of them, without running out of stack
        let mut many = b"\xff\xd8".to_vec();
        many.extend(b"\xff\xd0".repeat(1 << 21));
        assert_eq!(sniff(&many), None);
        assert_eq!(sniff(b"\xff\xd8\xff\xe0\x00"), None, "cut short");
        assert_eq!(sniff(b"plain text"), None);
        assert!(Image::new(png(0, 5)).is_err());
        assert!(Image::looks_like(&png(1, 1)) && !Image::looks_like(b"fn main"));
    }

    #[test]
    fn a_photo_turned_by_its_exif_reports_its_turned_size() {
        let rotated = include_bytes!("decode/testdata/rotated.jpg");
        assert_eq!(sniff(rotated), Some((Format::Jpeg, 37, 23, 6)));
        let image = Image::new(rotated.to_vec()).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (23, 37),
            "portrait, as it's meant to be seen"
        );
        // little-endian EXIF too, as many cameras write it
        let le = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x08\0\0\0";
        assert_eq!(exif_orientation(le), Some(8));
        assert_eq!(exif_orientation(b"Exif\0\0MM"), None, "cut short");
        assert_eq!(exif_orientation(b"XMP"), None);
    }

    #[test]
    fn which_terminals_can() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(
            support_in(env(&[("TERM", "xterm-kitty")])),
            Support::Yes(Protocol::Kitty)
        );
        assert_eq!(
            support_in(env(&[("TERM_PROGRAM", "ghostty")])),
            Support::Yes(Protocol::Kitty)
        );
        assert_eq!(
            support_in(env(&[("TERM_PROGRAM", "iTerm.app")])),
            Support::Yes(Protocol::Iterm)
        );
        assert_eq!(
            support_in(env(&[("TERM_PROGRAM", "WezTerm")])),
            Support::Yes(Protocol::Iterm)
        );
        assert_eq!(
            support_in(env(&[("KONSOLE_VERSION", "230800")])),
            Support::Yes(Protocol::Kitty)
        );
        assert_eq!(support_in(env(&[("TERM_PROGRAM", "Apple_Terminal")])), Support::No);
        assert_eq!(
            support_in(env(&[("TERM", "xterm-kitty"), ("TMUX", "/tmp/x")])),
            Support::No
        );
        assert_eq!(
            support_in(env(&[("TINSEL_IMAGES", "iterm")])),
            Support::Yes(Protocol::Iterm)
        );
        assert_eq!(
            support_in(env(&[("TERM", "xterm-kitty"), ("TINSEL_IMAGES", "none")])),
            Support::No
        );
    }

    #[test]
    fn kitty_takes_png_and_iterm_takes_them_all() {
        let kitty = Support::Yes(Protocol::Kitty);
        assert!(kitty.check_format(Format::Png).is_ok());
        // without the decoder kitty can't be sent a JPEG, and with it, it can
        if cfg!(feature = "decode") {
            assert!(kitty.check_format(Format::Jpeg).is_ok());
        } else {
            assert_eq!(
                kitty.check_format(Format::Jpeg).unwrap_err(),
                "this terminal shows PNG images, and this is a JPEG"
            );
        }
        assert!(Support::Yes(Protocol::Iterm).check_format(Format::Gif).is_ok());
    }

    #[test]
    fn pictures_fit_without_stretching_or_growing() {
        let area = Rect::new(10, 5, 40, 20);
        // 800x400 pixels in cells 10x20: 80x20 cells wanted, so halved to fit 40 wide
        assert_eq!(fit((800, 400), (10, 20), area), Rect::new(10, 10, 40, 10));
        // a tall one fits the height
        assert_eq!(fit((100, 1000), (10, 20), area), Rect::new(28, 5, 4, 20));
        // a small one stays its size, in the middle
        assert_eq!(fit((100, 40), (10, 20), area), Rect::new(25, 14, 10, 2));
    }

    #[test]
    fn kitty_is_sent_an_image_once_and_told_where_to_put_it() {
        let image = Image::new(png(2, 2)).unwrap();
        let mut shown = Shown::default();
        let at = Rect::new(3, 1, 4, 2);
        let first = shown.update(&[(at, image.clone())], Protocol::Kitty, (1600, 1000));
        assert!(first.contains(&format!("\x1b_Ga=t,f=100,t=d,i={},q=2,m=0;", image.id)));
        assert!(first.contains(&format!("\x1b[2;4H\x1b_Ga=p,i={},c=4,r=2,C=1,q=2\x1b\\", image.id)));
        assert!(!shown.changes(&[(at, image.clone())]));
        let moved = shown.update(&[(Rect::new(0, 0, 4, 2), image.clone())], Protocol::Kitty, (1600, 1000));
        assert!(moved.starts_with(&format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", image.id)));
        assert!(!moved.contains("a=t"), "kitty already has it");
        let gone = shown.update(&[], Protocol::Kitty, (1600, 1000));
        assert_eq!(
            gone,
            format!("\x1b_Ga=d,d=I,i={},q=2\x1b\\", image.id),
            "freed, as nothing shows it"
        );
        assert!(shown.is_empty());
        let back = shown.update(&[(at, image.clone())], Protocol::Kitty, (1600, 1000));
        assert!(back.contains("a=t,"), "so sent again when it's back");
    }

    #[test]
    fn big_images_go_to_kitty_in_pieces() {
        let mut bytes = png(1, 1);
        bytes.resize(10_000, 7);
        let image = Image::new(bytes).unwrap();
        let mut out = String::new();
        kitty_send(&mut out, &image, (1600, 1000));
        assert_eq!(out.matches("\x1b_G").count(), 4, "13336 base64 characters in 4096s");
        assert!(out.contains("m=1;") && out.ends_with("\x1b\\") && out.contains("\x1b_Gm=0;"));
    }

    #[cfg(feature = "decode")]
    #[test]
    fn a_big_picture_decodes_on_another_thread_and_waits_till_then() {
        // the test JPEG with a megabyte of comment segments before its picture
        let small = include_bytes!("decode/testdata/baseline.jpg");
        let mut big = small[..2].to_vec();
        for _ in 0..17 {
            big.extend([0xff, 0xfe, 0xff, 0xff]);
            big.extend([b'x'; 0xfffd]);
        }
        big.extend(&small[2..]);
        let image = Image::new(big).unwrap();
        let held = HOLD.lock().unwrap();
        assert_eq!(Support::Yes(Protocol::Kitty).check(&image), Ok(Protocol::Kitty));
        assert!(image.pending(), "still decoding");
        drop(held);
        // the thread finishes, and the picture is the one a small file gives
        assert_eq!(image.pixels().map(|p| (p.width, p.height)), Ok((37, 23)));
        assert!(!image.pending());
        let small = Image::new(small.to_vec()).unwrap();
        assert!(
            Support::Yes(Protocol::Kitty).check(&small).is_ok() && !small.pending(),
            "a small one right away"
        );
    }

    #[cfg(feature = "decode")]
    #[test]
    fn kitty_is_sent_a_jpeg_as_pixels_no_bigger_than_the_screen() {
        let image = Image::new(include_bytes!("decode/testdata/baseline.jpg").to_vec()).unwrap();
        assert_eq!(Support::Yes(Protocol::Kitty).check(&image), Ok(Protocol::Kitty));
        let (data, keys) = kitty_data(&image, (1600, 1000));
        assert_eq!(keys, "f=24,s=37,v=23", "solid, so three bytes a pixel");
        assert_eq!(data.len(), (37 * 23 * 3usize).div_ceil(3) * 4);
        assert_eq!(
            kitty_data(&image, (20, 10)).1,
            "f=24,s=16,v=10",
            "shrunk to what the screen holds"
        );
        assert!(image.ready_for(Protocol::Kitty) && image.ready_for(Protocol::Iterm));
        let broken = Image::new(include_bytes!("decode/testdata/baseline.jpg")[..300].to_vec()).unwrap();
        assert!(
            !broken.ready_for(Protocol::Kitty),
            "kitty would get nothing it can show"
        );
        assert!(broken.ready_for(Protocol::Iterm), "iTerm2 reads the file itself");
        assert!(
            Support::Yes(Protocol::Kitty)
                .check(&broken)
                .unwrap_err()
                .starts_with("can't read it")
        );
    }

    #[test]
    fn iterm_is_sent_the_file_where_it_goes() {
        let image = Image::new(png(2, 2)).unwrap();
        let out = Shown::default().update(&[(Rect::new(0, 2, 5, 3), image.clone())], Protocol::Iterm, (1600, 1000));
        assert!(out.starts_with("\x1b[3;1H\x1b]1337;File=inline=1;size=24;width=5;height=3;preserveAspectRatio=1:"));
        assert!(out.ends_with(&format!("{}\x07", crate::clipboard::base64(&image.bytes))));
    }
}

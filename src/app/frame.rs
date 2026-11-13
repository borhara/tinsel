//! The screen as a grid of cells. Everything in tinsel draws styled strings; a frame reads
//! their escape codes back into cells, so pieces can be placed anywhere, clipped, and drawn
//! over each other, and only the rows that changed are sent to the terminal.

use super::Rect;
use super::widgets::Widget;
use crate::block::Block;
use crate::style::Rgb;
use crate::text::char_width;

const BOLD: u8 = 1;
const DIM: u8 = 2;
const ITALIC: u8 = 4;
const UNDERLINE: u8 = 8;
const BLINK: u8 = 16;
const REVERSE: u8 = 32;

/// A colour as it arrived in an escape code, kept as-is so the theme and colour mode that
/// chose it aren't decided twice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Ink {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Ink {
    /// The colour a terminal shows for this ink, or `None` for its own default.
    fn rgb(self) -> Option<Rgb> {
        // the 16 named colours as most terminals draw them out of the box
        const NAMED: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (205, 0, 0),
            (0, 205, 0),
            (205, 205, 0),
            (0, 0, 238),
            (205, 0, 205),
            (0, 205, 205),
            (229, 229, 229),
            (127, 127, 127),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (92, 92, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        match self {
            Ink::Default => None,
            Ink::Rgb(r, g, b) => Some(Rgb(r, g, b)),
            Ink::Indexed(n @ 0..16) => {
                let (r, g, b) = NAMED[n as usize];
                Some(Rgb(r, g, b))
            }
            // a 6×6×6 cube, then 24 greys
            Ink::Indexed(n @ 16..232) => {
                let level = |i: u8| if i == 0 { 0 } else { 55 + 40 * i };
                let i = n - 16;
                Some(Rgb(level(i / 36), level(i / 6 % 6), level(i % 6)))
            }
            Ink::Indexed(n) => {
                let grey = 8 + 10 * (n - 232);
                Some(Rgb(grey, grey, grey))
            }
        }
    }
}

/// The page's own text and background colours, for the theme in use.
fn html_defaults() -> (Rgb, Rgb) {
    let back = if crate::style::theme() == crate::style::Theme::Light {
        Rgb(253, 250, 252)
    } else {
        Rgb(24, 20, 23)
    };
    (crate::style::shown(crate::palette::TEXT), back)
}

/// What a cell looks like on a page: its colours where they aren't the page's own, and the
/// attributes CSS can draw.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Look {
    ink: Option<Rgb>,
    paper: Option<Rgb>,
    attrs: u8,
}

impl Look {
    /// The inline CSS for it, empty when it looks like the page's own text.
    fn css(&self) -> String {
        let mut rules = Vec::new();
        if let Some(ink) = self.ink {
            rules.push(format!("color:{}", css(ink)));
        }
        if let Some(paper) = self.paper {
            rules.push(format!("background:{}", css(paper)));
        }
        for (bit, rule) in [
            (BOLD, "font-weight:bold"),
            (ITALIC, "font-style:italic"),
            (UNDERLINE, "text-decoration:underline"),
        ] {
            if self.attrs & bit != 0 {
                rules.push(rule.to_string());
            }
        }
        rules.join(";")
    }
}

/// A colour in CSS.
fn css(Rgb(r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Add a character to HTML text.
fn escape(c: char, out: &mut String) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        c => out.push(c),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Style {
    fg: Ink,
    bg: Ink,
    attrs: u8,
}

impl Style {
    /// Apply the parameters of one `ESC [ … m`.
    fn apply(&mut self, params: &str) {
        let mut it = params.split(';').map(|p| p.parse::<u16>().unwrap_or(0));
        while let Some(p) = it.next() {
            match p {
                0 => *self = Style::default(),
                1 => self.attrs |= BOLD,
                2 => self.attrs |= DIM,
                3 => self.attrs |= ITALIC,
                4 => self.attrs |= UNDERLINE,
                5 => self.attrs |= BLINK,
                7 => self.attrs |= REVERSE,
                22 => self.attrs &= !(BOLD | DIM),
                23 => self.attrs &= !ITALIC,
                24 => self.attrs &= !UNDERLINE,
                25 => self.attrs &= !BLINK,
                27 => self.attrs &= !REVERSE,
                30..=37 => self.fg = Ink::Indexed((p - 30) as u8),
                90..=97 => self.fg = Ink::Indexed((p - 90 + 8) as u8),
                40..=47 => self.bg = Ink::Indexed((p - 40) as u8),
                100..=107 => self.bg = Ink::Indexed((p - 100 + 8) as u8),
                39 => self.fg = Ink::Default,
                49 => self.bg = Ink::Default,
                38 | 48 => {
                    let ink = match it.next() {
                        Some(5) => Ink::Indexed(it.next().unwrap_or(0) as u8),
                        Some(2) => Ink::Rgb(
                            it.next().unwrap_or(0) as u8,
                            it.next().unwrap_or(0) as u8,
                            it.next().unwrap_or(0) as u8,
                        ),
                        _ => Ink::Default,
                    };
                    if p == 38 { self.fg = ink } else { self.bg = ink }
                }
                _ => {}
            }
        }
    }

    /// The escape code that sets exactly this style from scratch.
    fn sgr(&self, out: &mut String) {
        out.push_str("\x1b[0");
        for (bit, code) in [
            (BOLD, 1),
            (DIM, 2),
            (ITALIC, 3),
            (UNDERLINE, 4),
            (BLINK, 5),
            (REVERSE, 7),
        ] {
            if self.attrs & bit != 0 {
                out.push_str(&format!(";{code}"));
            }
        }
        for (ink, layer) in [(self.fg, 38), (self.bg, 48)] {
            match ink {
                Ink::Default => {}
                Ink::Indexed(n) if n < 8 => out.push_str(&format!(";{}", layer - 8 + n as u16)),
                Ink::Indexed(n) if n < 16 => out.push_str(&format!(";{}", layer + 52 + n as u16 - 8)),
                Ink::Indexed(n) => out.push_str(&format!(";{layer};5;{n}")),
                Ink::Rgb(r, g, b) => out.push_str(&format!(";{layer};2;{r};{g};{b}")),
            }
        }
        out.push('m');
    }

    /// How this style looks on a page whose own colours are `fg` and `bg`.
    fn look(&self, fg: Rgb, bg: Rgb) -> Look {
        let (mut ink, mut paper) = (self.fg.rgb().unwrap_or(fg), self.bg.rgb().unwrap_or(bg));
        if self.attrs & REVERSE != 0 {
            (ink, paper) = (paper, ink);
        }
        if self.attrs & DIM != 0 {
            ink = crate::style::mix(ink, paper, 0.5);
        }
        Look {
            ink: (ink != fg).then_some(ink),
            paper: (paper != bg).then_some(paper),
            attrs: self.attrs & (BOLD | ITALIC | UNDERLINE),
        }
    }

    /// Whether a space in this style looks the same as an empty cell.
    fn blank(&self) -> bool {
        self.bg == Ink::Default && self.attrs & (UNDERLINE | REVERSE) == 0
    }
}

/// One cell. A wide character fills its own cell and puts `'\0'` in the one after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    ch: char,
    style: Style,
}

const EMPTY: Cell = Cell {
    ch: ' ',
    style: Style {
        fg: Ink::Default,
        bg: Ink::Default,
        attrs: 0,
    },
};

/// What one frame of an [`App`](super::App) looks like. [`App::view`](super::App::view)
/// is handed an empty one and fills it with [`render`](Frame::render).
///
/// ```
/// use tinsel::app::{Frame, Panel};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut frame = Frame::new(20, 3);
/// frame.render(frame.area(), Panel::new("hi").body("hello"));
/// assert_eq!(frame.snapshot().plain(), "╭─ hi ─────────────╮\n│ hello            │\n╰──────────────────╯");
/// ```
#[derive(Clone, Debug)]
pub struct Frame {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    time: f32,
}

impl Frame {
    /// An empty frame `width` columns by `height` rows. Apps are given theirs; this is for
    /// tests and for drawing widgets on their own.
    pub fn new(width: usize, height: usize) -> Self {
        Frame {
            width,
            height,
            cells: vec![EMPTY; width * height],
            time: 0.0,
        }
    }

    /// The frame as it is `seconds` after the app started, for testing anything animated.
    ///
    /// ```
    /// use tinsel::app::{Frame, Panel};
    ///
    /// let draw = |t| {
    ///     let mut frame = Frame::new(12, 3).at(t);
    ///     frame.render(frame.area(), Panel::new("hi").focused(true));
    ///     frame.snapshot()
    /// };
    /// assert_eq!(draw(0.0).plain(), draw(1.0).plain());   // the same text…
    /// ```
    pub fn at(mut self, seconds: f32) -> Self {
        self.time = seconds;
        self
    }

    /// The whole screen.
    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// Seconds since the app started, for anything animated: pass it to
    /// [`Spinner::frame`](crate::Spinner::frame), [`shimmer`](crate::shimmer) and friends.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Draw a widget into part of the screen. Anything that doesn't fit is cut off.
    pub fn render(&mut self, area: Rect, widget: impl Widget) {
        widget.render(area.intersect(self.area()), self);
    }

    /// Write one line of styled text into row `row` of `area`, cut off at its right edge.
    /// Returns how many columns it took.
    pub fn print(&mut self, area: Rect, row: usize, text: &str) -> usize {
        let area = area.intersect(self.area());
        if row >= area.height {
            return 0;
        }
        self.write(area.x, area.y + row, area.x + area.width, text, 0)
    }

    /// Like [`print`](Frame::print), with the text's first `skip` columns left off: for
    /// anything scrolled sideways.
    pub(crate) fn print_from(&mut self, area: Rect, row: usize, text: &str, skip: usize) -> usize {
        let area = area.intersect(self.area());
        if row >= area.height {
            return 0;
        }
        self.write(area.x, area.y + row, area.x + area.width, text, skip)
    }

    /// Write each line of a block into `area`, one per row, from the top.
    pub fn lines(&mut self, area: Rect, block: &Block) {
        for (row, line) in block.lines().iter().take(area.height).enumerate() {
            self.print(area, row, line);
        }
    }

    /// Blank out part of the screen, so whatever's drawn there next sits on nothing.
    pub fn clear(&mut self, area: Rect) {
        self.fill(area, EMPTY);
    }

    /// Paint the cells behind part of the screen, keeping whatever's written there.
    pub fn tint(&mut self, area: Rect, bg: Rgb) {
        if crate::style::color_mode() == crate::style::ColorMode::None {
            return;
        }
        let mut style = Style::default();
        style.apply(&crate::style::sgr(bg, 48));
        let area = area.intersect(self.area());
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                self.cells[y * self.width + x].style.bg = style.bg;
            }
        }
    }

    /// Fade part of the screen into the background, so whatever's drawn on top of it (a
    /// [`Modal`](super::Modal), say) stands out. Without colour it's left as it is.
    ///
    /// ```
    /// use tinsel::app::Frame;
    ///
    /// let mut frame = Frame::new(10, 1);
    /// frame.print(frame.area(), 0, "behind");
    /// frame.dim(frame.area());
    /// assert_eq!(frame.snapshot().plain(), "behind");
    /// ```
    pub fn dim(&mut self, area: Rect) {
        if crate::style::color_mode() == crate::style::ColorMode::None {
            return;
        }
        let back = if crate::style::theme() == crate::style::Theme::Light {
            255.0
        } else {
            0.0
        };
        let fade = |ink: Ink, keep: f32| match ink {
            Ink::Rgb(r, g, b) => {
                let f = |c: u8| (c as f32 * keep + back * (1.0 - keep)).round() as u8;
                Ink::Rgb(f(r), f(g), f(b))
            }
            other => other,
        };
        let area = area.intersect(self.area());
        for y in area.y..area.y + area.height {
            for cell in &mut self.cells[y * self.width + area.x..y * self.width + area.x + area.width] {
                let s = &mut cell.style;
                // colours the frame knows are faded by hand; the terminal's own, with DIM
                if !matches!(s.fg, Ink::Rgb(..)) {
                    s.attrs |= DIM;
                }
                s.fg = fade(s.fg, 0.4);
                s.bg = fade(s.bg, 0.5);
            }
        }
    }

    fn fill(&mut self, area: Rect, cell: Cell) {
        let area = area.intersect(self.area());
        if area.is_empty() {
            return;
        }
        for y in area.y..area.y + area.height {
            self.cells[y * self.width + area.x..y * self.width + area.x + area.width].fill(cell);
        }
    }

    /// Write `text` from column `x` of row `y`, stopping before column `end`, leaving off
    /// its first `skip` columns.
    fn write(&mut self, x: usize, y: usize, end: usize, text: &str, mut skip: usize) -> usize {
        let row = y * self.width;
        let mut col = x;
        let mut style = Style::default();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                match chars.next() {
                    Some('[') => {
                        let mut params = String::new();
                        for c in chars.by_ref() {
                            if ('@'..='~').contains(&c) {
                                if c == 'm' {
                                    style.apply(&params);
                                }
                                break;
                            }
                            params.push(c);
                        }
                    }
                    // OSC (hyperlinks): there's no keeping them in a cell, so they're dropped
                    Some(']') => {
                        while let Some(c) = chars.next() {
                            if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
            let c = if c == '\t' { ' ' } else { c };
            let w = char_width(c);
            if c.is_control() || w == 0 {
                continue;
            }
            if skip > 0 {
                // half of a wide character scrolled off: the half still showing is a space
                if w > skip && col < end {
                    let style = self.over(row + col, style);
                    self.put(row, col, Cell { ch: ' ', style });
                    col += 1;
                }
                skip = skip.saturating_sub(w);
                continue;
            }
            if col + w > end {
                break;
            }
            let style = self.over(row + col, style);
            self.put(row, col, Cell { ch: c, style });
            if w == 2 {
                self.put(row, col + 1, Cell { ch: '\0', style });
            }
            col += w;
        }
        col - x
    }

    /// Text with no background of its own shows whatever's behind it, such as a tint.
    fn over(&self, at: usize, mut style: Style) -> Style {
        if style.bg == Ink::Default {
            style.bg = self.cells[at].style.bg;
        }
        style
    }

    /// Set one cell, tidying up any wide character it cuts in half.
    fn put(&mut self, row: usize, col: usize, cell: Cell) {
        let old = self.cells[row + col];
        if old.ch == '\0' && col > 0 {
            self.cells[row + col - 1].ch = ' ';
        }
        // even when the new cell is itself a wide character's tail: the old one's tail is one
        // further on, and would be left without its character
        if char_width(old.ch) == 2 && col + 1 < self.width {
            self.cells[row + col + 1].ch = ' ';
        }
        self.cells[row + col] = cell;
    }

    /// Row `y` as a styled string, without trailing blanks.
    fn row_text(&self, y: usize) -> String {
        let row = &self.cells[y * self.width..(y + 1) * self.width];
        let end = row
            .iter()
            .rposition(|c| !(c.ch == ' ' && c.style.blank()))
            .map_or(0, |i| i + 1);
        let mut out = String::new();
        let mut style = Style::default();
        for cell in &row[..end] {
            if cell.ch == '\0' {
                continue;
            }
            if cell.style != style {
                cell.style.sgr(&mut out);
                style = cell.style;
            }
            out.push(cell.ch);
        }
        if style != Style::default() {
            out.push_str("\x1b[0m");
        }
        out
    }

    /// The frame as text, a line per row, trailing blanks trimmed. `.plain()` it in tests.
    pub fn snapshot(&self) -> Block {
        (0..self.height).map(|y| self.row_text(y)).collect()
    }

    /// The frame as a web page that looks just like the terminal: a `<pre>` on tinsel's
    /// background for the [`theme`](crate::theme) in use, and each run of cells in the same
    /// style a `<span>` with its colours and its bold, dim, italic, underline or reverse. For
    /// screenshots in a README, or to see how something looks without a terminal. The colours
    /// are the ones the frame was drawn with, so draw it in colour (see
    /// [`set_color_mode`](crate::set_color_mode)) to see them. [`html_pre`](Frame::html_pre)
    /// is just the `<pre>`, to put in a page of your own.
    ///
    /// ```
    /// use tinsel::app::{Frame, Panel};
    ///
    /// let mut frame = Frame::new(20, 3);
    /// frame.render(frame.area(), Panel::new("hi").body("1 < 2"));
    /// let page = frame.html();
    /// assert!(page.starts_with("<!DOCTYPE html>") && page.contains("1 &lt; 2"));
    /// // std::fs::write("screenshot.html", page)?;
    /// ```
    pub fn html(&self) -> String {
        let (_, bg) = html_defaults();
        format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>tinsel</title>\n</head>\n\
             <body style=\"margin:0;padding:16px;background:{}\">\n{}\n</body>\n</html>\n",
            css(bg),
            self.html_pre()
        )
    }

    /// The frame as a `<pre>` element, styled inline so it can go anywhere in a page. See
    /// [`html`](Frame::html).
    ///
    /// ```
    /// use tinsel::app::Frame;
    ///
    /// let mut frame = Frame::new(4, 1);
    /// frame.print(frame.area(), 0, "a&b");
    /// let pre = frame.html_pre();
    /// assert!(pre.starts_with("<pre style=") && pre.ends_with("a&amp;b </pre>"));
    /// ```
    pub fn html_pre(&self) -> String {
        let (fg, bg) = html_defaults();
        let mut out = format!(
            "<pre style=\"margin:0;padding:8px 12px;display:inline-block;color:{};background:{};\
             font-family:ui-monospace,monospace;font-size:14px;line-height:1.25\">",
            css(fg),
            css(bg)
        );
        for y in 0..self.height {
            if y > 0 {
                out.push('\n');
            }
            let row = &self.cells[y * self.width..(y + 1) * self.width];
            // runs of cells that look the same
            let mut runs: Vec<(Look, String)> = Vec::new();
            for cell in row.iter().filter(|c| c.ch != '\0') {
                let look = cell.style.look(fg, bg);
                let mut text = String::new();
                escape(cell.ch, &mut text);
                if char_width(cell.ch) == 2 {
                    // fonts rarely draw wide characters exactly two columns across; make them
                    text = format!("<span style=\"display:inline-block;width:2ch\">{text}</span>");
                }
                // a space shows only its background (and underline), so joins any run on the same one
                // that isn't underlined
                let space = cell.ch == ' ' && look.attrs & UNDERLINE == 0;
                match runs.last_mut() {
                    Some((last, run))
                        if *last == look || (space && last.paper == look.paper && last.attrs & UNDERLINE == 0) =>
                    {
                        run.push_str(&text)
                    }
                    _ if space => runs.push((
                        Look {
                            paper: look.paper,
                            ..Look::default()
                        },
                        text,
                    )),
                    _ => runs.push((look, text)),
                }
            }
            for (look, text) in runs {
                match look.css() {
                    css if css.is_empty() => out.push_str(&text),
                    css => out.push_str(&format!("<span style=\"{css}\">{text}</span>")),
                }
            }
        }
        out.push_str("</pre>");
        out
    }

    /// What to send the terminal to turn `prev` (if it's the same size) into this frame.
    pub(crate) fn diff(&self, prev: Option<&Frame>) -> String {
        let prev = prev.filter(|p| p.width == self.width && p.height == self.height);
        let mut out = String::new();
        if self.width == 0 {
            return out;
        }
        for y in 0..self.height {
            let span = y * self.width..(y + 1) * self.width;
            if prev.is_some_and(|p| p.cells[span.clone()] == self.cells[span]) {
                continue;
            }
            out.push_str(&format!("\x1b[{};1H\x1b[0m", y + 1));
            out.push_str(&self.row_text(y));
            // with wrapping off, a full row leaves the cursor on its last cell, which erasing
            // to the end of the line would take with it
            let last = self.cells[(y + 1) * self.width - 1];
            if last.ch == ' ' && last.style.blank() {
                out.push_str("\x1b[K");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_styles_back_from_escape_codes() {
        let mut s = Style::default();
        s.apply("1;38;2;236;122;164");
        assert_eq!(s.fg, Ink::Rgb(236, 122, 164));
        assert_eq!(s.attrs, BOLD);
        s.apply("39;22;48;5;200");
        assert_eq!((s.fg, s.bg, s.attrs), (Ink::Default, Ink::Indexed(200), 0));
        let mut out = String::new();
        s.sgr(&mut out);
        assert_eq!(out, "\x1b[0;48;5;200m");
    }

    #[test]
    fn styles_round_trip_through_escape_codes() {
        let mut s = Style::default();
        s.apply("1;2;3;4;5;7;31;92;43;104");
        assert_eq!((s.fg, s.bg), (Ink::Indexed(10), Ink::Indexed(12)));
        let mut out = String::new();
        s.sgr(&mut out);
        assert_eq!(out, "\x1b[0;1;2;3;4;5;7;92;104m");
        let mut back = Style::default();
        back.apply(out.trim_start_matches("\x1b[").trim_end_matches('m'));
        assert_eq!(back, s);
        assert!(!s.blank());

        s.apply("22;23;24;25;27;39;49;38;5;3;48;9");
        assert_eq!((s.fg, s.bg, s.attrs), (Ink::Indexed(3), Ink::Default, 0));
        out.clear();
        s.sgr(&mut out);
        assert_eq!(out, "\x1b[0;33m");
        s.apply("0");
        assert_eq!(s, Style::default());
        s.apply("41;48;2;1;2;3");
        assert_eq!(s.bg, Ink::Rgb(1, 2, 3));
        s.apply("4");
        assert!(!s.blank());
    }

    #[test]
    fn writes_text_and_blocks_and_drops_what_it_cant_keep() {
        let mut f = Frame::new(8, 3).at(1.5);
        assert_eq!(f.time(), 1.5);
        // a hyperlink keeps its text, tabs become spaces, other controls and combining marks go
        let n = f.print(f.area(), 0, "\x1b]8;;https://x.y\x1b\\li\x1b]8;;\x07nk\ta\u{7}e\u{301}");
        assert_eq!((f.row_text(0).as_str(), n), ("link ae", 7));
        assert_eq!(f.print(f.area(), 5, "past the bottom"), 0);
        f.lines(
            Rect::new(2, 1, 6, 1),
            &Block::from(vec!["one".to_string(), "two".to_string()]),
        );
        assert_eq!(f.row_text(1), "  one");
        f.tint(f.area(), crate::palette::ROSE);
        f.clear(Rect::new(0, 0, 4, 1));
        assert_eq!(f.row_text(0), "     ae");
        assert_eq!(Frame::new(0, 2).diff(None), "");
    }

    #[test]
    fn clips_and_keeps_wide_characters_whole() {
        let mut f = Frame::new(6, 2);
        f.print(f.area(), 0, "\x1b[38;2;1;2;3mab日本\x1b[39m");
        assert_eq!(crate::text::strip_ansi(&f.row_text(0)), "ab日本");
        // a wide character that would straddle the edge is left off
        f.print(Rect::new(0, 1, 2, 1), 0, "a日");
        assert_eq!(f.row_text(1), "a");
        // writing over half of one blanks the other half
        f.print(Rect::new(3, 0, 3, 1), 0, "x");
        assert_eq!(crate::text::strip_ansi(&f.row_text(0)), "ab x本");
    }

    #[test]
    fn text_scrolled_sideways_loses_its_first_columns() {
        let mut f = Frame::new(6, 3);
        f.print_from(f.area(), 0, "\x1b[1mabcdef\x1b[0m", 2);
        assert_eq!(f.row_text(0), "\x1b[0;1mcdef\x1b[0m");
        // half a wide character scrolled off leaves a space where its other half was
        f.print_from(f.area(), 1, "a日本", 2);
        assert_eq!(f.row_text(1), " 本");
        assert_eq!(f.print_from(f.area(), 3, "gone", 0), 0);
    }

    #[test]
    fn wide_characters_drawn_over_each_other_stay_aligned() {
        let mut f = Frame::new(6, 1);
        f.print(f.area(), 0, "a日|");
        f.print(f.area(), 0, "本");
        assert_eq!(f.row_text(0), "本 |");
    }

    #[test]
    fn only_sends_rows_that_changed() {
        let mut a = Frame::new(4, 3);
        a.print(a.area(), 0, "one");
        let mut b = a.clone();
        b.print(b.area(), 2, "two");
        let diff = b.diff(Some(&a));
        assert!(diff.contains("\x1b[3;1H") && !diff.contains("\x1b[1;1H"));
        assert_eq!(b.diff(Some(&b)), "");
    }

    #[test]
    fn clearing_off_screen_does_nothing() {
        let mut f = Frame::new(10, 3);
        f.clear(Rect::new(15, 2, 5, 1));
        f.render(Rect::new(12, 0, 5, 5), super::super::Panel::new("gone"));
        assert_eq!(f.snapshot().plain(), "\n\n");
    }

    #[test]
    fn full_rows_keep_their_last_cell() {
        let mut f = Frame::new(3, 2);
        f.print(f.area(), 0, "abc");
        f.print(f.area(), 1, "ab");
        let diff = f.diff(None);
        assert!(diff.contains("abc\x1b[2;1H"), "{diff:?}");
        assert!(diff.ends_with("ab\x1b[K"), "{diff:?}");
    }
}

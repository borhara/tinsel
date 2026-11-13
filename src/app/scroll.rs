//! Text too long for its area, scrolled through: a log, a help screen, a file.

use std::cell::Cell;

use super::widgets::{scrollbar, wrap};
use super::{Event, Frame, Key, MouseKind, Rect, Widget};
use crate::style::paint;
use crate::style::palette::MUTED;

/// How far a [`ScrollView`] is scrolled. Keep one in your app for each view, pass it events
/// in [`event`](super::App::event), and hand it to the view in [`view`](super::App::view).
///
/// One made with [`following`](Scroll::following) keeps the end in view as lines are
/// added, like `tail -f`. Scrolling up stops that, so what's being read stays put, and
/// scrolling back to the end starts it again.
///
/// ```
/// use tinsel::app::{Event, Frame, Key, Scroll, ScrollView};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
/// let mut scroll = Scroll::new();
/// let mut frame = Frame::new(10, 3);
/// frame.render(frame.area(), ScrollView::new(&lines).scroll(&scroll));
/// scroll.event(&Event::Key(Key::PageDown));
/// frame.render(frame.area(), ScrollView::new(&lines).scroll(&scroll));
/// assert_eq!(scroll.top(), 2);
/// assert!(frame.snapshot().plain().starts_with("line 3"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Scroll {
    follows: bool,
    following: bool,
    // the first line showing, and what the last drawing found, so the next move knows how
    // far it can go
    top: Cell<usize>,
    rows: Cell<usize>,
    total: Cell<usize>,
}

impl Scroll {
    /// Scrolled to the top.
    pub fn new() -> Self {
        Scroll::default()
    }

    /// Scrolled to the end, and kept there as lines are added, until scrolled up.
    pub fn following() -> Self {
        Scroll {
            follows: true,
            following: true,
            ..Scroll::default()
        }
    }

    /// The first line showing, counted from 0.
    pub fn top(&self) -> usize {
        self.top.get()
    }

    /// Whether the end is being kept in view: made [`following`](Scroll::following), and
    /// not scrolled up since. Good for a "following" mark in a panel's status.
    pub fn is_following(&self) -> bool {
        self.following
    }

    /// Scroll so `line` is the first showing (as far as there's text below it to fill the
    /// view). Stops following the end.
    pub fn scroll_to(&mut self, line: usize) {
        self.top.set(line);
        self.following = false;
    }

    /// Scroll to the end, and follow it if this scroll follows.
    pub fn scroll_to_end(&mut self) {
        self.top.set(self.total.get().saturating_sub(self.rows.get()));
        self.following = self.follows;
    }

    /// Scroll in response to an event: ↑↓ and j/k a line, Page Up/Down and Space a screen,
    /// Home/End (or g/G) to the ends, and the scroll wheel three lines. Returns whether it
    /// moved, judging by how much the view showed when it was last drawn.
    ///
    /// ```
    /// use tinsel::app::{Event, Key, Scroll};
    ///
    /// let mut scroll = Scroll::following();
    /// scroll.event(&Event::Key(Key::Up));
    /// assert!(!scroll.is_following());
    /// ```
    pub fn event(&mut self, event: &Event) -> bool {
        let (top, rows) = (self.top.get(), self.rows.get());
        let last = self.total.get().saturating_sub(rows);
        let page = rows.saturating_sub(1).max(1);
        let to = match event {
            Event::Key(Key::Up | Key::Char('k')) => top.saturating_sub(1),
            Event::Key(Key::Down | Key::Char('j')) => top + 1,
            Event::Key(Key::PageUp) => top.saturating_sub(page),
            Event::Key(Key::PageDown | Key::Char(' ')) => top + page,
            Event::Key(Key::Home | Key::Char('g')) => 0,
            Event::Key(Key::End | Key::Char('G')) => last,
            Event::Mouse(m) if m.kind == MouseKind::ScrollUp => top.saturating_sub(3),
            Event::Mouse(m) if m.kind == MouseKind::ScrollDown => top + 3,
            _ => return false,
        };
        let to = to.min(last);
        let up = matches!(
            event,
            Event::Key(Key::Up | Key::Char('k') | Key::PageUp | Key::Home | Key::Char('g'))
        ) || matches!(event, Event::Mouse(m) if m.kind == MouseKind::ScrollUp);
        let was_following = self.following;
        self.top.set(to);
        // any way up stops following, even with nowhere to go, and down to the end starts it again
        self.following = self.follows && to == last && !up;
        to != top || was_following != self.following
    }
}

/// Lines to scroll through, with a scrollbar when there are more than fit: a log, a help
/// screen, a file. Lines are cut at the edge unless [`wrap`](ScrollView::wrap)ped, and
/// styled text is fine. Without a [`Scroll`], it shows the top.
///
/// ```
/// use tinsel::app::{Frame, Scroll, ScrollView};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let log = ["starting", "listening on :8080", "GET /health 200"];
/// let tail = Scroll::following();
/// let mut frame = Frame::new(20, 2);
/// frame.render(frame.area(), ScrollView::new(log).scroll(&tail));
/// assert_eq!(frame.snapshot().plain(), "listening on :8080 │\nGET /health 200    ┃");
/// ```
#[derive(Clone, Debug)]
pub struct ScrollView<'a> {
    lines: Vec<String>,
    scroll: Option<&'a Scroll>,
    wrap: bool,
    empty: String,
}

impl<'a> ScrollView<'a> {
    /// A view of these lines. A [`Block`](crate::Block) works too, as `block.lines()`.
    pub fn new<S: Into<String>>(lines: impl IntoIterator<Item = S>) -> Self {
        ScrollView {
            lines: lines.into_iter().map(Into::into).collect(),
            scroll: None,
            wrap: false,
            empty: "nothing here yet".into(),
        }
    }

    /// How far it's scrolled.
    pub fn scroll(mut self, scroll: &'a Scroll) -> Self {
        self.scroll = Some(scroll);
        self
    }

    /// Word-wrap long lines to the width, instead of cutting them off.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// What to say when there are no lines: "nothing here yet" unless changed.
    pub fn empty(mut self, text: impl Into<String>) -> Self {
        self.empty = text.into();
        self
    }

    /// The lines as drawn `cols` wide.
    fn rows(&self, cols: usize) -> Vec<String> {
        if !self.wrap {
            return self.lines.clone();
        }
        self.lines.iter().flat_map(|l| wrap(l, cols.max(1))).collect()
    }
}

impl Widget for ScrollView<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        if self.lines.is_empty() {
            frame.print(area, 0, &paint(MUTED, &self.empty));
            return;
        }
        let h = area.height;
        // wrapped to leave room for a scrollbar only if one turns out to be needed
        let mut rows = self.rows(area.width);
        let overflow = rows.len() > h;
        if overflow && self.wrap {
            rows = self.rows(area.width - 1);
        }
        let last = rows.len().saturating_sub(h);
        let top = match self.scroll {
            Some(s) if s.following => last,
            Some(s) => s.top.get().min(last),
            None => 0,
        };
        if let Some(s) = self.scroll {
            s.top.set(top);
            s.rows.set(h);
            s.total.set(rows.len());
        }
        let text = Rect::new(area.x, area.y, area.width - usize::from(overflow), h);
        for (row, line) in rows[top..].iter().take(h).enumerate() {
            frame.print(text, row, line);
        }
        if overflow {
            scrollbar(frame, Rect::new(area.x + area.width - 1, area.y, 1, h), top, rows.len());
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let widest = self.lines.iter().map(|l| crate::text::width(l)).max().unwrap_or(0);
        Some((widest, self.lines.len().max(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(view: ScrollView, w: usize, h: usize) -> String {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(w, h);
        f.render(f.area(), view);
        f.snapshot().plain()
    }

    fn numbered(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("{i}")).collect()
    }

    fn key(k: Key) -> Event {
        Event::Key(k)
    }

    #[test]
    fn moves_by_lines_pages_and_ends() {
        let lines = numbered(20);
        let mut s = Scroll::new();
        assert!(!s.event(&key(Key::Down)), "nothing drawn yet, so nowhere to go");
        drawn(ScrollView::new(&lines).scroll(&s), 6, 5);
        let wheel = |kind| Event::Mouse(super::super::Mouse { kind, x: 0, y: 0 });
        let steps = [
            (key(Key::Down), 1),
            (key(Key::Char('j')), 2),
            (key(Key::Char('k')), 1),
            (key(Key::PageDown), 5),
            (key(Key::Char(' ')), 9),
            (key(Key::PageUp), 5),
            (key(Key::End), 15),
            (key(Key::Down), 15),
            (key(Key::Char('g')), 0),
            (key(Key::Char('G')), 15),
            (key(Key::Home), 0),
            (key(Key::Up), 0),
            (wheel(MouseKind::ScrollDown), 3),
            (wheel(MouseKind::ScrollUp), 0),
        ];
        for (event, want) in steps {
            s.event(&event);
            assert_eq!(s.top(), want, "after {event:?}");
        }
        assert!(!s.event(&Event::Tick));
        assert!(!s.event(&wheel(MouseKind::Click)));
        s.scroll_to(18);
        assert_eq!(
            drawn(ScrollView::new(&lines).scroll(&s), 6, 5).lines().next(),
            Some("16   │")
        );
        assert_eq!(s.top(), 15, "kept to what fills the view");
    }

    #[test]
    fn following_keeps_the_end_in_view_until_scrolled_up() {
        let mut lines = numbered(10);
        let mut s = Scroll::following();
        assert!(drawn(ScrollView::new(&lines).scroll(&s), 6, 3).starts_with("8"));
        lines.extend(numbered(2).into_iter().map(|l| format!("new {l}")));
        assert!(drawn(ScrollView::new(&lines).scroll(&s), 6, 3).ends_with("new 2┃"));

        // up stops it, even at the end; down to the end starts it again
        assert!(s.event(&key(Key::Up)) && !s.is_following());
        assert!(s.event(&key(Key::Down)) && s.is_following());
        assert!(s.event(&key(Key::Char('k'))) && !s.is_following());
        s.scroll_to_end();
        assert!(s.is_following());
        let wheel = Event::Mouse(super::super::Mouse {
            kind: MouseKind::ScrollUp,
            x: 0,
            y: 0,
        });
        drawn(ScrollView::new(&lines).scroll(&s), 6, 3);
        assert!(s.event(&wheel) && !s.is_following());
        lines.push("newer".into());
        assert!(!drawn(ScrollView::new(&lines).scroll(&s), 6, 3).contains("newer"));

        // a scroll made to stay put never follows
        let mut still = Scroll::new();
        still.scroll_to_end();
        assert!(!still.is_following());
    }

    #[test]
    fn wraps_long_lines_and_leaves_room_for_the_bar() {
        let lines = ["one two three", "four"];
        assert_eq!(drawn(ScrollView::new(lines).wrap(), 8, 3), "one two\nthree\nfour");
        assert_eq!(drawn(ScrollView::new(lines).wrap(), 8, 2), "one two┃\nthree  │");
        assert_eq!(drawn(ScrollView::new(lines), 8, 2), "one two\nfour");
    }

    #[test]
    fn says_when_theres_nothing_and_knows_its_size() {
        let none: [&str; 0] = [];
        assert_eq!(drawn(ScrollView::new(none), 20, 1), "nothing here yet");
        assert_eq!(drawn(ScrollView::new(none).empty("no logs"), 20, 1), "no logs");
        assert_eq!(drawn(ScrollView::new(["x"]), 0, 0), "");
        assert_eq!(ScrollView::new(["ab", "abcd"]).size(), Some((4, 2)));
        assert_eq!(ScrollView::new(none).size(), Some((0, 1)));
    }
}

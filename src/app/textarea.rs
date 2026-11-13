//! Several lines of text to type into, inside an app.

use std::cell::Cell;

use super::field::caret;
use super::{Event, Frame, Key, Rect, Widget};
use crate::style::palette::{BERRY, MUTED, SOFT, TEXT, TRACK};
use crate::style::{italic, mix, paint};
use crate::text::char_width;

/// A box to type several lines into: a message, notes, a description. Like [`Input`](super::Input),
/// keep one in your app, pass it every event while it has focus, and read what's typed with
/// [`value`](TextArea::value). Enter starts a new line, pasted text keeps its line breaks,
/// long lines wrap, and the box scrolls to keep the cursor in view.
///
/// ```
/// use tinsel::app::{Event, Frame, Key, TextArea};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut notes = TextArea::new().placeholder("what changed?");
/// notes.focus(true);
/// notes.event(&Event::Paste("fix the build\n".into()));
/// for c in "and the docs".chars() {
///     notes.event(&Event::Key(Key::Char(c)));
/// }
/// assert_eq!(notes.value(), "fix the build\nand the docs");
/// assert_eq!(notes.cursor(), (1, 12));
/// ```
#[derive(Clone, Debug)]
pub struct TextArea {
    lines: Vec<Vec<char>>,
    /// The cursor: a line, and a character in it.
    row: usize,
    col: usize,
    /// The column ↑↓ aim for, so passing a short line doesn't lose it.
    goal: Option<usize>,
    placeholder: String,
    focused: bool,
    /// The first visual row showing, kept between drawings so the view only moves when the
    /// cursor would leave it.
    top: Cell<usize>,
}

impl Default for TextArea {
    fn default() -> Self {
        TextArea::new()
    }
}

impl TextArea {
    /// An empty box, without focus.
    pub fn new() -> Self {
        TextArea {
            lines: vec![Vec::new()],
            row: 0,
            col: 0,
            goal: None,
            placeholder: String::new(),
            focused: false,
            top: Cell::new(0),
        }
    }

    /// Quiet text shown while the box is empty, saying what goes in it.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// What's been typed, its lines joined with `\n`.
    pub fn value(&self) -> String {
        let lines: Vec<String> = self.lines.iter().map(|l| l.iter().collect()).collect();
        lines.join("\n")
    }

    /// Replace what's typed, with the cursor at the end.
    pub fn set_value(&mut self, value: &str) {
        self.lines = vec![Vec::new()];
        (self.row, self.col) = (0, 0);
        self.insert(value);
    }

    /// Empty the box.
    pub fn clear(&mut self) {
        self.set_value("");
    }

    /// Where the cursor is: the line, and the character in it, both counted from 0.
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// Whether this is where keys go. Only a focused box takes keys or shows its cursor.
    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Whether the box has focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Type text at the cursor, starting new lines at its line breaks. Tabs become four
    /// spaces, and other control characters are left out.
    fn insert(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => self.split_line(),
                '\t' => (0..4).for_each(|_| self.put(' ')),
                c if c.is_control() => {}
                c => self.put(c),
            }
        }
    }

    fn put(&mut self, c: char) {
        self.lines[self.row].insert(self.col, c);
        self.col += 1;
    }

    fn split_line(&mut self) {
        let rest = self.lines[self.row].split_off(self.col);
        self.row += 1;
        self.col = 0;
        self.lines.insert(self.row, rest);
    }

    /// Move to `row`, as near the goal column as that line allows.
    fn go_to_row(&mut self, row: usize) {
        let goal = *self.goal.get_or_insert(self.col);
        self.row = row;
        self.col = goal.min(self.lines[row].len());
    }

    /// Type, delete or move the cursor, if the box has focus and the event is one it uses:
    /// characters and pastes, Enter for a new line, Backspace and Delete (joining lines at
    /// their ends), ←→↑↓, Page Up/Down ten lines, Home and End (or Ctrl-A and Ctrl-E) for
    /// the line's ends, Ctrl-U and Ctrl-K to clear the line before or after the cursor, and
    /// Ctrl-W to delete a word. Returns whether it used the event, so Esc and Tab are left
    /// for the app, to leave the box with.
    ///
    /// ```
    /// use tinsel::app::{Event, Key, TextArea};
    ///
    /// let mut text = TextArea::new();
    /// text.focus(true);
    /// text.set_value("one\ntwo");
    /// text.event(&Event::Key(Key::Home));
    /// text.event(&Event::Key(Key::Backspace));
    /// assert_eq!(text.value(), "onetwo");
    /// assert!(!text.event(&Event::Key(Key::Esc)));
    /// ```
    pub fn event(&mut self, event: &Event) -> bool {
        if !self.focused {
            return false;
        }
        let key = match event {
            Event::Key(key) => *key,
            Event::Paste(text) => {
                self.insert(text);
                self.goal = None;
                return true;
            }
            _ => return false,
        };
        let (row, col) = (self.row, self.col);
        let last = self.lines.len() - 1;
        match key {
            Key::Up if row > 0 => return self.moved_to_row(row - 1),
            Key::Down if row < last => return self.moved_to_row(row + 1),
            Key::PageUp => return self.moved_to_row(row.saturating_sub(10)),
            Key::PageDown => return self.moved_to_row((row + 10).min(last)),
            Key::Up => self.col = 0,
            Key::Down => self.col = self.lines[row].len(),
            Key::Char(c) => self.put(c),
            Key::Enter => self.split_line(),
            Key::Backspace if col > 0 => {
                self.lines[row].remove(col - 1);
                self.col -= 1;
            }
            Key::Backspace if row > 0 => {
                let line = self.lines.remove(row);
                self.row -= 1;
                self.col = self.lines[self.row].len();
                self.lines[self.row].extend(line);
            }
            Key::Delete if col < self.lines[row].len() => {
                self.lines[row].remove(col);
            }
            Key::Delete if row < last => {
                let next = self.lines.remove(row + 1);
                self.lines[row].extend(next);
            }
            Key::Left if col > 0 => self.col -= 1,
            Key::Left if row > 0 => {
                self.row -= 1;
                self.col = self.lines[self.row].len();
            }
            Key::Right if col < self.lines[row].len() => self.col += 1,
            Key::Right if row < last => (self.row, self.col) = (row + 1, 0),
            Key::Home | Key::Ctrl('a') => self.col = 0,
            Key::End | Key::Ctrl('e') => self.col = self.lines[row].len(),
            Key::Ctrl('u') => {
                self.lines[row].drain(..col);
                self.col = 0;
            }
            Key::Ctrl('k') => self.lines[row].truncate(col),
            Key::Ctrl('w') => {
                let line = &mut self.lines[row];
                let mut start = col;
                while start > 0 && line[start - 1] == ' ' {
                    start -= 1;
                }
                while start > 0 && line[start - 1] != ' ' {
                    start -= 1;
                }
                line.drain(start..col);
                self.col = start;
            }
            Key::Backspace | Key::Delete | Key::Left | Key::Right => {}
            _ => return false,
        }
        self.goal = None;
        true
    }

    fn moved_to_row(&mut self, row: usize) -> bool {
        self.go_to_row(row);
        true
    }

    /// The lines as drawn `width` wide, broken by characters, and the visual row and column
    /// the cursor is at. A cursor at the end of a full row goes to the start of the next.
    fn layout(&self, width: usize) -> (Vec<String>, (usize, usize)) {
        let width = width.max(1);
        let mut rows = Vec::new();
        let mut cursor = (0, 0);
        for (i, line) in self.lines.iter().enumerate() {
            let (mut row, mut x) = (String::new(), 0);
            for (j, &c) in line.iter().chain([' '].iter()).enumerate() {
                let w = char_width(c);
                if x + w > width && x > 0 {
                    rows.push(std::mem::take(&mut row));
                    x = 0;
                }
                if (i, j) == (self.row, self.col) {
                    cursor = (rows.len(), x);
                }
                if j < line.len() {
                    row.push(c);
                    x += w;
                }
            }
            rows.push(row);
        }
        (rows, cursor)
    }
}

impl Widget for TextArea {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        if self.focused {
            frame.tint(area, mix(TRACK, BERRY, 0.18));
        }
        if self.lines.len() == 1 && self.lines[0].is_empty() {
            frame.print(area, 0, &paint(MUTED, &italic(&self.placeholder)));
            if self.focused {
                frame.print(area, 0, &caret(self.placeholder.chars().next().unwrap_or(' ')));
            }
            return;
        }
        let (rows, (cursor_row, cursor_x)) = self.layout(area.width);
        let h = area.height;
        let top = self.top.get().min(cursor_row).max((cursor_row + 1).saturating_sub(h));
        self.top.set(top);
        let colour = if self.focused { TEXT } else { SOFT };
        for (n, line) in rows.iter().skip(top).take(h).enumerate() {
            frame.print(area, n, &paint(colour, line));
        }
        if self.focused {
            let under = self.lines[self.row].get(self.col).copied().unwrap_or(' ');
            let line = area.row(cursor_row - top);
            let at = Rect::new(line.x + cursor_x, line.y, line.width.saturating_sub(cursor_x), 1);
            frame.print(at, 0, &caret(under));
        }
    }

    fn size(&self) -> Option<(usize, usize)> {
        let widest = self
            .lines
            .iter()
            .map(|l| l.iter().map(|&c| char_width(c)).sum::<usize>());
        Some((widest.max().unwrap_or(0).max(20), self.lines.len().max(3)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &mut TextArea, keys: &[Key]) {
        for k in keys {
            text.event(&Event::Key(*k));
        }
    }

    fn drawn(text: &TextArea, w: usize, h: usize) -> String {
        crate::set_color_mode(crate::ColorMode::None);
        let mut f = Frame::new(w, h);
        f.render(f.area(), text);
        f.snapshot().plain()
    }

    fn focused(value: &str) -> TextArea {
        let mut text = TextArea::new();
        text.focus(true);
        text.set_value(value);
        text
    }

    #[test]
    fn types_breaks_and_joins_lines() {
        let mut text = TextArea::new();
        typed(&mut text, &[Key::Char('x')]);
        assert_eq!(text.value(), "", "no focus, no typing");
        assert!(!text.event(&Event::Paste("x".into())));
        text.focus(true);
        typed(
            &mut text,
            &[Key::Char('a'), Key::Enter, Key::Char('b'), Key::Left, Key::Backspace],
        );
        assert_eq!((text.value().as_str(), text.cursor()), ("ab", (0, 1)));
        typed(&mut text, &[Key::Enter, Key::Up, Key::End, Key::Delete]);
        assert_eq!((text.value().as_str(), text.cursor()), ("ab", (0, 1)));
        typed(&mut text, &[Key::Delete, Key::Delete, Key::Home, Key::Backspace]);
        assert_eq!(text.value(), "a", "nothing to delete at either end");
        typed(&mut text, &[Key::End]);
        assert!(text.event(&Event::Paste("\tb\r\x07\nc".into())));
        assert_eq!(text.value(), "a    b\nc");
        assert!(!text.event(&Event::Tick) && !text.event(&Event::Key(Key::Tab)));
        text.clear();
        assert_eq!((text.value().as_str(), text.cursor()), ("", (0, 0)));
    }

    #[test]
    fn moves_across_lines_keeping_its_column() {
        let mut text = focused("long line\nab\nanother line");
        assert_eq!(text.cursor(), (2, 12));
        typed(&mut text, &[Key::Up]);
        assert_eq!(text.cursor(), (1, 2), "as far as the short line goes");
        typed(&mut text, &[Key::Up]);
        assert_eq!(text.cursor(), (0, 9), "and back out to where it was aiming");
        typed(&mut text, &[Key::Up]);
        assert_eq!(text.cursor(), (0, 0), "up from the top goes to the start");
        typed(&mut text, &[Key::PageDown]);
        assert_eq!(text.cursor(), (2, 0));
        typed(&mut text, &[Key::Down]);
        assert_eq!(text.cursor(), (2, 12), "down from the bottom goes to the end");
        typed(&mut text, &[Key::PageUp]);
        assert_eq!(text.cursor(), (0, 9));
        typed(&mut text, &[Key::Right]);
        assert_eq!(text.cursor(), (1, 0), "right from a line's end goes to the next");
        typed(&mut text, &[Key::Left]);
        assert_eq!(text.cursor(), (0, 9), "and left comes back");
        typed(&mut text, &[Key::Ctrl('a'), Key::Left]);
        assert_eq!(text.cursor(), (0, 0), "nowhere left of the start");
        typed(&mut text, &[Key::Ctrl('e')]);
        assert_eq!(text.cursor(), (0, 9));
    }

    #[test]
    fn clears_parts_of_a_line() {
        let mut text = focused("keep\ndeploy my app");
        typed(&mut text, &[Key::Ctrl('w')]);
        assert_eq!(text.value(), "keep\ndeploy my ");
        typed(&mut text, &[Key::Ctrl('w'), Key::Left, Key::Ctrl('k')]);
        assert_eq!(text.value(), "keep\ndeploy");
        typed(&mut text, &[Key::Left, Key::Ctrl('u')]);
        assert_eq!((text.value().as_str(), text.cursor()), ("keep\ny", (1, 0)));
    }

    #[test]
    fn wraps_and_scrolls_to_keep_the_cursor_in_view() {
        let mut text = focused("abcdefgh\nij");
        assert_eq!(drawn(&text, 5, 4), "abcde\nfgh\nij \n");
        typed(&mut text, &[Key::Up, Key::End]);
        assert_eq!(text.layout(4).1, (2, 0), "past a full row, the cursor starts the next");
        // three rows, two showing: the view follows the cursor, and stays put when it can
        assert_eq!(drawn(&text, 5, 2), "abcde\nfgh ");
        typed(&mut text, &[Key::Down]);
        assert_eq!(drawn(&text, 5, 2), "fgh\nij ");
        typed(&mut text, &[Key::Up]);
        assert_eq!(drawn(&text, 5, 2), "fgh \nij");
        typed(&mut text, &[Key::Ctrl('a')]);
        assert_eq!(drawn(&text, 5, 2), "abcde\nfgh");
        text.focus(false);
        assert_eq!(drawn(&text, 5, 2), "abcde\nfgh");
        assert_eq!(drawn(&text, 0, 0), "");
    }

    #[test]
    fn shows_its_placeholder_and_wants_a_little_room() {
        let empty = TextArea::new().placeholder("notes");
        assert_eq!(drawn(&empty, 10, 1), "notes");
        let mut lit = empty.clone();
        lit.focus(true);
        assert_eq!(drawn(&lit, 10, 1), "notes");
        assert_eq!(TextArea::default().size(), Some((20, 3)));
        assert_eq!(focused(&"x".repeat(30)).size(), Some((30, 3)));
    }
}

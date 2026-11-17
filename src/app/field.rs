//! A line of text to type into, inside an app.

use super::{CursorShape, Event, Frame, Key, Mods, NamedKey, Rect, Widget};
use crate::style::palette::{BERRY, MUTED, ROSE, SOFT, TEXT, TRACK};
use crate::style::{ColorMode, Rgb, color_mode, italic, mix, paint, paint_on};
use crate::text::char_width;

/// A box to type into: a filter, a search, a name. Keep one in your app, pass it every
/// event while it has focus, and read what's typed with [`value`](TextInput::value).
///
/// ```
/// use tinsel::app::{Event, Frame, TextInput, Key};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut filter = TextInput::new().prompt("/").placeholder("filter by name");
/// let mut frame = Frame::new(20, 1);
/// frame.render(frame.area(), &filter);
/// assert_eq!(frame.snapshot().plain(), "/ filter by name");
///
/// filter.focus(true);
/// for c in "api".chars() {
///     filter.handle(&Event::Key(Key::Char(c)));
/// }
/// assert_eq!(filter.value(), "api");
/// ```
#[derive(Clone, Debug)]
pub struct TextInput {
    value: Vec<char>,
    at: usize,
    prompt: String,
    placeholder: String,
    focused: bool,
    is_word: fn(char) -> bool,
    history: Option<History>,
    /// What was typed before going back through the history, to come back to.
    draft: Option<Vec<char>>,
    terminal_cursor: bool,
}

/// What a key or paste did to a [`TextInput`], from [`TextInput::edit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Edit {
    /// The text changed.
    Changed,
    /// Only the cursor moved, or the key was one the box uses that had nothing to do
    /// (Backspace at the start).
    Moved,
    /// Enter was pressed: the text is ready.
    Submitted,
    /// Esc was pressed: whatever it's for is off.
    Cancelled,
    /// Not something the box uses, or it hasn't focus: the app's to handle.
    Ignored,
}

/// Lines entered before, to go back to with ↑ and ↓ in a [`TextInput`]: a command line's
/// history. A line entered again moves to the newest, blank lines aren't kept, and the
/// oldest go once there are more than its [`limit`](History::limit).
///
/// ```
/// use tinsel::app::History;
///
/// let mut history = History::from(vec!["ls".to_string(), "git status".into()]);
/// history.push("ls");
/// assert_eq!(history.entries(), ["git status", "ls"]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    entries: Vec<String>,
    /// Which entry is showing while going back through them.
    at: Option<usize>,
    limit: Option<usize>,
}

impl From<Vec<String>> for History {
    fn from(entries: Vec<String>) -> Self {
        History {
            entries,
            at: None,
            limit: None,
        }
    }
}

impl History {
    /// No lines yet, and no limit.
    pub fn new() -> Self {
        History::default()
    }

    /// Keep only the newest `lines`.
    #[must_use]
    pub fn limit(mut self, lines: usize) -> Self {
        self.limit = Some(lines);
        self.trim();
        self
    }

    /// Remember a line entered, as the newest.
    pub fn push(&mut self, line: &str) {
        self.at = None;
        if line.trim().is_empty() {
            return;
        }
        self.entries.retain(|e| e != line);
        self.entries.push(line.to_string());
        self.trim();
    }

    fn trim(&mut self) {
        if let Some(limit) = self.limit {
            let over = self.entries.len().saturating_sub(limit);
            self.entries.drain(..over);
        }
    }

    /// Every line, oldest first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// One line further back, or `None` with nothing to go back to: the newest at first,
    /// then older ones, stopping at the oldest.
    pub fn back(&mut self) -> Option<&str> {
        let last = self.entries.len().checked_sub(1)?;
        let at = self.at.map_or(last, |i| i.saturating_sub(1));
        self.at = Some(at);
        Some(&self.entries[at])
    }

    /// One line nearer now, or `None` once past the newest, back where it started.
    pub fn forward(&mut self) -> Option<&str> {
        let at = self.at? + 1;
        self.at = (at < self.entries.len()).then_some(at);
        self.at.map(|i| self.entries[i].as_str())
    }

    /// Whether it's partway back through its lines.
    pub fn is_browsing(&self) -> bool {
        self.at.is_some()
    }

    /// Stop going back through its lines, so the next [`back`](History::back) starts from
    /// the newest again.
    pub fn reset(&mut self) {
        self.at = None;
    }
}

/// Whether a character is part of a word, unless [`TextInput::word`] says otherwise:
/// anything but a space.
fn not_space(c: char) -> bool {
    !c.is_whitespace()
}

impl Default for TextInput {
    fn default() -> Self {
        TextInput::new()
    }
}

impl TextInput {
    /// An empty box, without focus.
    pub fn new() -> Self {
        TextInput {
            value: Vec::new(),
            at: 0,
            prompt: "›".into(),
            placeholder: String::new(),
            focused: false,
            is_word: not_space,
            history: None,
            draft: None,
            terminal_cursor: false,
        }
    }

    /// Which characters make up a word, for Ctrl-W and moving a word at a time: anything
    /// but a space unless changed. Deleting a word back takes the spaces before the cursor,
    /// then a run of word characters, or else a run of the others.
    ///
    /// ```
    /// use tinsel::app::{Event, Key, TextInput};
    ///
    /// let mut path = TextInput::new().word(|c| c.is_alphanumeric());
    /// path.focus(true);
    /// path.set_value("src/app/main");
    /// path.handle(&Event::Key(Key::Ctrl('w')));
    /// assert_eq!(path.value(), "src/app/");
    /// ```
    #[must_use]
    pub fn word(mut self, is_word: fn(char) -> bool) -> Self {
        self.is_word = is_word;
        self
    }

    /// Lines entered before, to go back through with ↑ and ↓. Enter adds the line to it.
    /// Going forward past the newest brings back what was being typed.
    #[must_use]
    pub fn with_history(mut self, history: History) -> Self {
        self.history = Some(history);
        self
    }

    /// The lines entered before, if it has a [history](TextInput::with_history), with
    /// those entered since: to keep for next time.
    pub fn history(&self) -> Option<&History> {
        self.history.as_ref()
    }

    /// Show the terminal's own cursor, a bar, where typing goes, rather than lighting up
    /// the character there: it blinks if the terminal's does, and sits where screen readers
    /// look.
    #[must_use]
    pub fn terminal_cursor(mut self, on: bool) -> Self {
        self.terminal_cursor = on;
        self
    }

    /// What sits before the text: `›` unless set. `/` suits a filter.
    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Quiet text shown while the box is empty, saying what goes in it.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// What's been typed.
    pub fn value(&self) -> String {
        self.value.iter().collect()
    }

    /// Replace what's typed, with the cursor at the end.
    pub fn set_value(&mut self, value: &str) {
        self.value = value.chars().collect();
        self.at = self.value.len();
    }

    /// Where the cursor is, in characters from the start: 0 before the first, the length
    /// after the last.
    pub fn cursor(&self) -> usize {
        self.at
    }

    /// Put the cursor before character `at` (counted in characters), or at the end if
    /// there aren't that many.
    ///
    /// ```
    /// use tinsel::app::TextInput;
    ///
    /// let mut input = TextInput::new();
    /// input.set_value("helo");
    /// input.set_cursor(3);
    /// input.insert_str("l");
    /// assert_eq!((input.value().as_str(), input.cursor()), ("hello", 4));
    /// ```
    pub fn set_cursor(&mut self, at: usize) {
        self.at = at.min(self.value.len());
    }

    /// Put text in at the cursor, as if typed, focused or not: line breaks and tabs become
    /// spaces and other control characters are left out, so it stays one line.
    pub fn insert_str(&mut self, text: &str) {
        let text = text.chars().filter_map(|c| match c {
            '\n' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        });
        let before = self.value.len();
        self.value.splice(self.at..self.at, text);
        self.at += self.value.len() - before;
    }

    /// Empty the box.
    pub fn clear(&mut self) {
        self.set_value("");
    }

    /// Whether this is where keys go. Only a focused box takes keys or shows its cursor.
    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Whether the box has focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Type, delete or move the cursor, if the box has focus and the event is a key it uses:
    /// characters, Backspace and Delete, ←→, Home and End (or Ctrl-A and Ctrl-E), Ctrl-U
    /// and Ctrl-K to clear before or after the cursor, Ctrl-W or Alt-Backspace to delete a
    /// word, and Alt or Ctrl with ←→ to move by words. Pasted text goes in at the cursor, on one line: its line breaks and tabs become spaces.
    /// Returns whether it used the event, so Enter, Esc and Tab are left for the app.
    ///
    /// ```
    /// use tinsel::app::{Event, TextInput, Key};
    ///
    /// let mut name = TextInput::new();
    /// name.focus(true);
    /// name.set_value("my-ap");
    /// name.handle(&Event::Key(Key::Char('p')));
    /// name.handle(&Event::Key(Key::Home));
    /// name.handle(&Event::Key(Key::Delete));
    /// assert_eq!(name.value(), "y-app");
    /// assert!(!name.handle(&Event::Key(Key::Enter)));
    /// ```
    pub fn handle(&mut self, event: &Event) -> bool {
        if matches!(event, Event::Key(Key::Enter | Key::Esc)) {
            return false;
        }
        matches!(self.edit(event), Edit::Changed | Edit::Moved)
    }

    /// Like [`handle`](TextInput::handle), saying what happened: whether the text changed
    /// (to filter again), only the cursor moved, Enter or Esc was pressed, or the event is
    /// the app's. With a [history](TextInput::with_history), ↑ and ↓ go back through it,
    /// and Enter adds the line to it.
    ///
    /// ```
    /// use tinsel::app::{Edit, Event, Key, TextInput};
    ///
    /// let mut search = TextInput::new();
    /// search.focus(true);
    /// assert_eq!(search.edit(&Event::Key(Key::Char('a'))), Edit::Changed);
    /// assert_eq!(search.edit(&Event::Key(Key::Left)), Edit::Moved);
    /// assert_eq!(search.edit(&Event::Key(Key::Enter)), Edit::Submitted);
    /// assert_eq!(search.edit(&Event::Key(Key::Tab)), Edit::Ignored);
    /// ```
    pub fn edit(&mut self, event: &Event) -> Edit {
        if !self.focused {
            return Edit::Ignored;
        }
        let key = match event {
            Event::Key(key) => *key,
            Event::Paste(text) => {
                self.insert_str(text);
                return Edit::Changed;
            }
            _ => return Edit::Ignored,
        };
        let (before, at) = (self.value.clone(), self.at);
        match key {
            Key::Enter => {
                if let Some(h) = self.history.as_mut() {
                    h.push(&self.value.iter().collect::<String>());
                }
                self.draft = None;
                return Edit::Submitted;
            }
            Key::Esc => {
                if let Some(h) = self.history.as_mut() {
                    h.reset();
                }
                self.draft = None;
                return Edit::Cancelled;
            }
            Key::Up | Key::Down if self.history.is_some() => {
                let history = self.history.as_mut().expect("just checked");
                if !history.is_browsing() {
                    self.draft = Some(self.value.clone());
                }
                let line = if key == Key::Up {
                    history.back().map(str::to_string)
                } else {
                    history.forward().map(str::to_string)
                };
                match line {
                    Some(line) => self.value = line.chars().collect(),
                    None if key == Key::Down => self.value = self.draft.take().unwrap_or_default(),
                    None => {}
                }
                self.at = self.value.len();
            }
            Key::Char(c) => {
                self.value.insert(at, c);
                self.at += 1;
            }
            Key::Backspace if at > 0 => {
                self.value.remove(at - 1);
                self.at -= 1;
            }
            Key::Delete if at < self.value.len() => {
                self.value.remove(at);
            }
            Key::Left => self.at = at.saturating_sub(1),
            Key::Right => self.at = (at + 1).min(self.value.len()),
            key if edit_line(&mut self.value, &mut self.at, key, self.is_word) => {}
            Key::Backspace | Key::Delete => {}
            _ => return Edit::Ignored,
        }
        if self.value != before {
            Edit::Changed
        } else {
            Edit::Moved
        }
    }

    /// The column the cursor is in when the box is drawn at `area`, and its row: where
    /// typing goes on the screen.
    ///
    /// ```
    /// use tinsel::app::{Rect, TextInput};
    ///
    /// let mut input = TextInput::new();
    /// input.set_value("abc");
    /// assert_eq!(input.cursor_cell(Rect::new(4, 2, 20, 1)), (9, 2));
    /// ```
    pub fn cursor_cell(&self, area: Rect) -> (usize, usize) {
        let lead = crate::text::width(&self.prompt) + 1;
        let room = area.width.saturating_sub(lead);
        let (_, x) = self.scrolled(room);
        ((area.x + lead + x).min((area.x + area.width).saturating_sub(1)), area.y)
    }

    /// Which character the box shows first, with `room` columns for the text, so the
    /// cursor stays in view, and how far in the cursor is then.
    fn scrolled(&self, room: usize) -> (usize, usize) {
        // back from the cursor as far as fits, wide character and all, one at a time
        let under = self.value.get(self.at).map_or(1, |&c| char_width(c).max(1));
        let mut start = self.at;
        let mut x = 0;
        while start > 0 && x + char_width(self.value[start - 1]) + under <= room {
            start -= 1;
            x += char_width(self.value[start]);
        }
        (start, x)
    }
}

impl Widget for TextInput {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() {
            return;
        }
        let line = area.row(0);
        let lead = format!("{} ", self.prompt);
        let lead_w = crate::text::width(&lead);
        if self.focused {
            frame.tint(line, mix(TRACK, BERRY, 0.18));
        }
        frame.print(line, 0, &paint(if self.focused { ROSE } else { MUTED }, &lead));
        let room = line.width.saturating_sub(lead_w);
        let field = Rect::new(line.x + lead_w, line.y, room, 1);

        if self.value.is_empty() {
            let hint = paint(MUTED, &italic(&self.placeholder));
            frame.print(field, 0, &hint);
            if self.focused && self.terminal_cursor {
                frame.set_cursor(field.x, field.y, CursorShape::Bar);
            } else if self.focused {
                let under = self.placeholder.chars().next().unwrap_or(' ');
                frame.print(field, 0, &caret(under));
            }
            return;
        }

        // scrolled so the cursor (one column past the end, at the end) stays in view
        let (start, x) = self.scrolled(room);
        // only as much as fits, so a long value costs no more than a short one
        let mut used = 0;
        let shown: String = self.value[start..]
            .iter()
            .take_while(|&&c| {
                used += char_width(c);
                used <= room
            })
            .collect();
        frame.print(field, 0, &paint(if self.focused { TEXT } else { SOFT }, &shown));
        if self.focused && self.terminal_cursor {
            frame.set_cursor(field.x + x, field.y, CursorShape::Bar);
        } else if self.focused {
            let under = self.value.get(self.at).copied().unwrap_or(' ');
            frame.print(
                Rect::new(field.x + x, field.y, room.saturating_sub(x), 1),
                0,
                &caret(under),
            );
        }
    }
}

/// The keys an [`TextInput`] and a [`TextArea`](super::TextArea) edit a line with alike, on
/// `line` with the cursor `at`: Home and Ctrl-A to the start, End and Ctrl-E to the end,
/// Ctrl-U and Ctrl-K to cut what's before or after, Ctrl-W or Alt-Backspace to cut the word
/// before, and Alt or Ctrl with ←→ (or Alt-B and Alt-F) to move a word at a time, words
/// being what `is_word` says. Returns whether `key` was one of them.
pub(super) fn edit_line(line: &mut Vec<char>, at: &mut usize, key: Key, is_word: fn(char) -> bool) -> bool {
    let space = |c: char| c.is_whitespace();
    // past any spaces, then a run of word characters, or else of anything but
    let word_start = |line: &[char], mut i: usize| {
        while i > 0 && space(line[i - 1]) {
            i -= 1;
        }
        let word = i > 0 && is_word(line[i - 1]);
        while i > 0 && !space(line[i - 1]) && is_word(line[i - 1]) == word {
            i -= 1;
        }
        i
    };
    let word_end = |line: &[char], mut i: usize| {
        while i < line.len() && space(line[i]) {
            i += 1;
        }
        let word = i < line.len() && is_word(line[i]);
        while i < line.len() && !space(line[i]) && is_word(line[i]) == word {
            i += 1;
        }
        i
    };
    let by_word = |m: Mods| m.ctrl() || m.alt();
    match key {
        Key::Home | Key::Ctrl('a') => *at = 0,
        Key::End | Key::Ctrl('e') => *at = line.len(),
        Key::Ctrl('u') => {
            line.drain(..*at);
            *at = 0;
        }
        Key::Ctrl('k') => line.truncate(*at),
        Key::Ctrl('w') | Key::Mod(Mods::ALT, NamedKey::Backspace) => {
            let start = word_start(line, *at);
            line.drain(start..*at);
            *at = start;
        }
        Key::Alt('b') => *at = word_start(line, *at),
        Key::Mod(m, NamedKey::Left) if by_word(m) => *at = word_start(line, *at),
        Key::Alt('f') => *at = word_end(line, *at),
        Key::Mod(m, NamedKey::Right) if by_word(m) => *at = word_end(line, *at),
        _ => return false,
    }
    true
}

/// The character under the cursor, lit up: on rose in colour, reversed without.
pub(super) fn caret(c: char) -> String {
    if color_mode() == ColorMode::None {
        return format!("\x1b[7m{c}\x1b[27m");
    }
    paint_on(Rgb(40, 30, 36), ROSE, &c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(input: &mut TextInput, keys: &[Key]) {
        for k in keys {
            input.handle(&Event::Key(*k));
        }
    }

    fn drawn(input: &TextInput, w: usize) -> String {
        crate::set_color_mode(ColorMode::None);
        let mut f = Frame::new(w, 1);
        f.render(f.area(), input);
        f.snapshot().plain()
    }

    #[test]
    fn edits_and_moves() {
        let mut input = TextInput::new();
        typed(&mut input, &[Key::Char('x')]);
        assert_eq!(input.value(), "", "no focus, no typing");
        input.focus(true);
        input.set_value("deploy my app");
        typed(&mut input, &[Key::Ctrl('w')]);
        assert_eq!(input.value(), "deploy my ");
        typed(
            &mut input,
            &[Key::Ctrl('w'), Key::Ctrl('a'), Key::Right, Key::Ctrl('k')],
        );
        assert_eq!(input.value(), "d");
        typed(&mut input, &[Key::Backspace, Key::Backspace, Key::Delete, Key::Left]);
        assert_eq!((input.value().as_str(), input.at), ("", 0));
        input.set_value("abc");
        typed(&mut input, &[Key::Left, Key::Ctrl('u'), Key::Ctrl('e'), Key::Char('!')]);
        assert_eq!(input.value(), "c!");
        input.set_value("one two  three");
        let ctrl = |k| Key::Mod(Mods::CTRL, k);
        typed(&mut input, &[Key::Alt('b'), ctrl(NamedKey::Left), Key::Char('_')]);
        assert_eq!(input.value(), "one _two  three");
        typed(&mut input, &[ctrl(NamedKey::Right), Key::Alt('f'), Key::Char('!')]);
        assert_eq!(input.value(), "one _two  three!");
        typed(&mut input, &[Key::Mod(Mods::ALT, NamedKey::Backspace)]);
        assert_eq!(input.value(), "one _two  ");
        assert!(!input.handle(&Event::Tick) && !input.handle(&Event::Key(Key::Tab)));
        input.clear();
        assert!(input.value().is_empty() && input.is_focused());
    }

    #[test]
    fn a_paste_goes_in_at_the_cursor_on_one_line() {
        let mut input = TextInput::new();
        let paste = Event::Paste("b\nc\td\x07".into());
        assert!(!input.handle(&paste), "no focus, no pasting");
        input.focus(true);
        input.set_value("a!");
        typed(&mut input, &[Key::Left]);
        assert!(input.handle(&paste));
        assert_eq!(input.value(), "ab c d!");
        typed(&mut input, &[Key::Char('?')]);
        assert_eq!(input.value(), "ab c d?!");
    }

    #[test]
    fn long_text_scrolls_to_keep_the_cursor_in_view() {
        let mut input = TextInput::default();
        input.focus(true);
        input.set_value("abcdefghij");
        assert_eq!(drawn(&input, 8), "› fghij ", "the cursor sits past the end");
        typed(&mut input, &[Key::Home]);
        assert_eq!(drawn(&input, 8), "› abcdef");
        input.focus(false);
        assert_eq!(drawn(&input, 20), "› abcdefghij");
        assert_eq!(drawn(&TextInput::new(), 0), "");

        // wide characters scroll a whole one at a time, and a long value shows only its end
        input.focus(true);
        input.set_value(&("x".repeat(20_000) + "日本語"));
        assert_eq!(drawn(&input, 10), "› x日本語 ");
        typed(&mut input, &[Key::Left]);
        assert_eq!(drawn(&input, 10), "› xx日本語");
    }

    #[test]
    fn the_cursor_shows_without_colour_too() {
        crate::set_color_mode(ColorMode::None);
        let mut input = TextInput::new().placeholder("name");
        input.focus(true);
        let mut f = Frame::new(10, 1);
        f.render(f.area(), &input);
        assert!(f.snapshot().to_string().contains("\x1b[0;7mn"), "{:?}", f.snapshot());
    }

    #[test]
    fn deletes_forwards_and_lights_the_caret_in_colour() {
        let mut input = TextInput::new();
        input.focus(true);
        input.set_value("abc");
        typed(&mut input, &[Key::Home, Key::Delete]);
        assert_eq!(input.value(), "bc");
        let lit = crate::style::with_color_mode(ColorMode::TrueColor, || caret('x'));
        // on a background, in whatever accent and theme the tests run in
        assert!(lit.contains("48;2;") && lit.contains('x'), "{lit:?}");
    }

    #[test]
    fn an_empty_field_without_focus_shows_its_placeholder() {
        let input = TextInput::new().placeholder("name");
        assert!(drawn(&input, 20).contains("name"));
    }

    #[test]
    fn edits_say_what_they_did() {
        let mut input = TextInput::new();
        let key = |k| Event::Key(k);
        assert_eq!(input.edit(&key(Key::Char('a'))), Edit::Ignored, "no focus");
        input.focus(true);
        assert_eq!(input.edit(&key(Key::Char('a'))), Edit::Changed);
        assert_eq!(input.edit(&key(Key::Home)), Edit::Moved);
        assert_eq!(
            input.edit(&key(Key::Backspace)),
            Edit::Moved,
            "nothing before the cursor"
        );
        assert_eq!(input.edit(&Event::Paste("bc".into())), Edit::Changed);
        assert_eq!(input.edit(&key(Key::Esc)), Edit::Cancelled);
        assert_eq!(input.edit(&key(Key::Enter)), Edit::Submitted);
        assert_eq!(input.edit(&key(Key::Up)), Edit::Ignored, "no history to go through");
        assert_eq!(input.edit(&Event::Tick), Edit::Ignored);
        assert_eq!((input.value().as_str(), input.cursor()), ("bca", 2));
        input.set_cursor(99);
        input.insert_str("d\ne");
        assert_eq!(input.value(), "bcad e");
    }

    #[test]
    fn history_goes_back_and_comes_forward_to_what_was_typed() {
        let mut input = TextInput::new().with_history(History::from(vec!["one".into(), "two".into()]).limit(3));
        input.focus(true);
        let press = |input: &mut TextInput, k| input.edit(&Event::Key(k));
        input.set_value("dra");
        press(&mut input, Key::Up);
        assert_eq!(input.value(), "two");
        press(&mut input, Key::Up);
        press(&mut input, Key::Up);
        assert_eq!(input.value(), "one", "stops at the oldest");
        press(&mut input, Key::Down);
        assert_eq!(input.value(), "two");
        press(&mut input, Key::Down);
        assert_eq!(
            (input.value().as_str(), input.cursor()),
            ("dra", 3),
            "back to the draft"
        );
        assert_eq!(press(&mut input, Key::Down), Edit::Moved);
        input.set_value("three");
        assert_eq!(press(&mut input, Key::Enter), Edit::Submitted);
        input.set_value("one");
        press(&mut input, Key::Enter);
        assert_eq!(input.history().unwrap().entries(), ["two", "three", "one"]);
        input.set_value("four");
        press(&mut input, Key::Enter);
        assert_eq!(
            input.history().unwrap().entries(),
            ["three", "one", "four"],
            "kept to its limit"
        );
        let mut blank = History::new();
        blank.push("  ");
        assert!(blank.entries().is_empty() && blank.back().is_none() && blank.forward().is_none());
    }

    #[test]
    fn words_are_what_the_box_says() {
        let mut input = TextInput::new().word(|c| c.is_alphanumeric() || c == '_');
        input.focus(true);
        input.set_value("call(my_arg");
        typed(&mut input, &[Key::Ctrl('w')]);
        assert_eq!(input.value(), "call(");
        typed(&mut input, &[Key::Ctrl('w')]);
        assert_eq!(input.value(), "call");
        typed(&mut input, &[Key::Alt('b')]);
        assert_eq!(input.cursor(), 0);
        typed(&mut input, &[Key::Alt('f')]);
        assert_eq!(input.cursor(), 4);
    }

    #[test]
    fn the_terminal_cursor_can_show_where_typing_goes() {
        let mut input = TextInput::new().terminal_cursor(true);
        input.focus(true);
        let mut f = Frame::new(10, 1);
        f.render(f.area(), &input);
        assert_eq!(f.cursor().map(|c| (c.x, c.y)), Some((2, 0)));
        input.set_value("abc");
        let mut f = Frame::new(10, 1);
        f.render(f.area(), &input);
        assert_eq!(f.cursor().map(|c| (c.x, c.shape)), Some((5, CursorShape::Bar)));
        assert_eq!(input.cursor_cell(f.area()), (5, 0));
    }
}

//! A line of text to type into, inside an app.

use super::{Event, Frame, Key, Mods, NamedKey, Rect, Widget};
use crate::style::palette::{BERRY, MUTED, ROSE, SOFT, TEXT, TRACK};
use crate::style::{ColorMode, Rgb, color_mode, italic, mix, paint, paint_bg};
use crate::text::char_width;

/// A box to type into: a filter, a search, a name. Keep one in your app, pass it every
/// event while it has focus, and read what's typed with [`value`](Input::value).
///
/// ```
/// use tinsel::app::{Event, Frame, Input, Key};
///
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// let mut filter = Input::new().prompt("/").placeholder("filter by name");
/// let mut frame = Frame::new(20, 1);
/// frame.render(frame.area(), &filter);
/// assert_eq!(frame.snapshot().plain(), "/ filter by name");
///
/// filter.focus(true);
/// for c in "api".chars() {
///     filter.event(&Event::Key(Key::Char(c)));
/// }
/// assert_eq!(filter.value(), "api");
/// ```
#[derive(Clone, Debug)]
pub struct Input {
    value: Vec<char>,
    at: usize,
    prompt: String,
    placeholder: String,
    focused: bool,
}

impl Default for Input {
    fn default() -> Self {
        Input::new()
    }
}

impl Input {
    /// An empty box, without focus.
    pub fn new() -> Self {
        Input {
            value: Vec::new(),
            at: 0,
            prompt: "›".into(),
            placeholder: String::new(),
            focused: false,
        }
    }

    /// What sits before the text: `›` unless set. `/` suits a filter.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Quiet text shown while the box is empty, saying what goes in it.
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
    /// use tinsel::app::{Event, Input, Key};
    ///
    /// let mut name = Input::new();
    /// name.focus(true);
    /// name.set_value("my-ap");
    /// name.event(&Event::Key(Key::Char('p')));
    /// name.event(&Event::Key(Key::Home));
    /// name.event(&Event::Key(Key::Delete));
    /// assert_eq!(name.value(), "y-app");
    /// assert!(!name.event(&Event::Key(Key::Enter)));
    /// ```
    pub fn event(&mut self, event: &Event) -> bool {
        if !self.focused {
            return false;
        }
        let key = match event {
            Event::Key(key) => key,
            Event::Paste(text) => {
                let text = text.chars().filter_map(|c| match c {
                    '\n' | '\t' => Some(' '),
                    c if c.is_control() => None,
                    c => Some(c),
                });
                let before = self.value.len();
                self.value.splice(self.at..self.at, text);
                self.at += self.value.len() - before;
                return true;
            }
            _ => return false,
        };
        let at = self.at;
        match *key {
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
            key if edit_line(&mut self.value, &mut self.at, key) => {}
            Key::Backspace | Key::Delete => {}
            _ => return false,
        }
        true
    }
}

impl Widget for Input {
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
            if self.focused {
                let under = self.placeholder.chars().next().unwrap_or(' ');
                frame.print(field, 0, &caret(under));
            }
            return;
        }

        // scrolled so the cursor (one column past the end, at the end) stays in view, wide
        // character and all: back from the cursor as far as fits, one character at a time
        let under = self.value.get(self.at).map_or(1, |&c| char_width(c).max(1));
        let mut start = self.at;
        let mut x = 0;
        while start > 0 && x + char_width(self.value[start - 1]) + under <= room {
            start -= 1;
            x += char_width(self.value[start]);
        }
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
        if self.focused {
            let under = self.value.get(self.at).copied().unwrap_or(' ');
            frame.print(
                Rect::new(field.x + x, field.y, room.saturating_sub(x), 1),
                0,
                &caret(under),
            );
        }
    }
}

/// The keys an [`Input`] and a [`TextArea`](super::TextArea) edit a line with alike, on
/// `line` with the cursor `at`: Home and Ctrl-A to the start, End and Ctrl-E to the end,
/// Ctrl-U and Ctrl-K to cut what's before or after, Ctrl-W or Alt-Backspace to cut the word
/// before, and Alt or Ctrl with ←→ (or Alt-B and Alt-F) to move a word at a time. Returns
/// whether `key` was one of them.
pub(super) fn edit_line(line: &mut Vec<char>, at: &mut usize, key: Key) -> bool {
    let word_start = |line: &[char], mut i: usize| {
        while i > 0 && line[i - 1] == ' ' {
            i -= 1;
        }
        while i > 0 && line[i - 1] != ' ' {
            i -= 1;
        }
        i
    };
    let word_end = |line: &[char], mut i: usize| {
        while i < line.len() && line[i] == ' ' {
            i += 1;
        }
        while i < line.len() && line[i] != ' ' {
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
    paint_bg(Rgb(40, 30, 36), ROSE, &c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(input: &mut Input, keys: &[Key]) {
        for k in keys {
            input.event(&Event::Key(*k));
        }
    }

    fn drawn(input: &Input, w: usize) -> String {
        crate::set_color_mode(ColorMode::None);
        let mut f = Frame::new(w, 1);
        f.render(f.area(), input);
        f.snapshot().plain()
    }

    #[test]
    fn edits_and_moves() {
        let mut input = Input::new();
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
        assert!(!input.event(&Event::Tick) && !input.event(&Event::Key(Key::Tab)));
        input.clear();
        assert!(input.value().is_empty() && input.is_focused());
    }

    #[test]
    fn a_paste_goes_in_at_the_cursor_on_one_line() {
        let mut input = Input::new();
        let paste = Event::Paste("b\nc\td\x07".into());
        assert!(!input.event(&paste), "no focus, no pasting");
        input.focus(true);
        input.set_value("a!");
        typed(&mut input, &[Key::Left]);
        assert!(input.event(&paste));
        assert_eq!(input.value(), "ab c d!");
        typed(&mut input, &[Key::Char('?')]);
        assert_eq!(input.value(), "ab c d?!");
    }

    #[test]
    fn long_text_scrolls_to_keep_the_cursor_in_view() {
        let mut input = Input::default();
        input.focus(true);
        input.set_value("abcdefghij");
        assert_eq!(drawn(&input, 8), "› fghij ", "the cursor sits past the end");
        typed(&mut input, &[Key::Home]);
        assert_eq!(drawn(&input, 8), "› abcdef");
        input.focus(false);
        assert_eq!(drawn(&input, 20), "› abcdefghij");
        assert_eq!(drawn(&Input::new(), 0), "");

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
        let mut input = Input::new().placeholder("name");
        input.focus(true);
        let mut f = Frame::new(10, 1);
        f.render(f.area(), &input);
        assert!(f.snapshot().to_string().contains("\x1b[0;7mn"), "{:?}", f.snapshot());
    }

    #[test]
    fn deletes_forwards_and_lights_the_caret_in_colour() {
        let mut input = Input::new();
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
        let input = Input::new().placeholder("name");
        assert!(drawn(&input, 20).contains("name"));
    }
}

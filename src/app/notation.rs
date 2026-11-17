//! Keys written down as vim writes them, `<C-a>`, `<Esc>`, `<A-x>`, and drawn as the symbols
//! on a Mac's menus, `⌃A`.

use std::fmt;
use std::str::FromStr;

use super::input::{Key, Mods, NamedKey};

/// Why some text isn't a key, from [`Key`]'s [`FromStr`]: it holds the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParseKeyError(String);

impl fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} isn't a key", self.0)
    }
}

impl std::error::Error for ParseKeyError {}

/// The names a key can go by between `<` and `>`, in lowercase, with what each is.
const NAMES: [(&str, Key); 22] = [
    ("esc", Key::Esc),
    ("cr", Key::Enter),
    ("enter", Key::Enter),
    ("return", Key::Enter),
    ("bs", Key::Backspace),
    ("backspace", Key::Backspace),
    ("del", Key::Delete),
    ("delete", Key::Delete),
    ("tab", Key::Tab),
    ("up", Key::Up),
    ("down", Key::Down),
    ("left", Key::Left),
    ("right", Key::Right),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("lt", Key::Char('<')),
    ("gt", Key::Char('>')),
    ("space", Key::Char(' ')),
    ("bar", Key::Char('|')),
    ("bslash", Key::Char('\\')),
];

/// What's between `<` and `>`: modifiers like `C-` then a name or a character.
fn named(inside: &str) -> Option<Key> {
    let mut mods = Mods::NONE;
    let mut rest = inside;
    // a modifier is a letter and a dash with something after it, so `<C-->` is Ctrl and `-`
    loop {
        let mut chars = rest.chars();
        let held = match (chars.next(), chars.next()) {
            (Some(m), Some('-')) if !chars.as_str().is_empty() => match m.to_ascii_lowercase() {
                'c' => Mods::CTRL,
                'a' | 'm' => Mods::ALT,
                's' => Mods::SHIFT,
                _ => break,
            },
            _ => break,
        };
        mods |= held;
        rest = chars.as_str();
    }
    let lower = rest.to_ascii_lowercase();
    let key = NAMES
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, key)| *key)
        .or_else(|| {
            let n: u8 = lower.strip_prefix('f')?.parse().ok()?;
            (1..=24).contains(&n).then_some(Key::F(n))
        });
    let mut chars = rest.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    match (key, single) {
        // a character needs a modifier to be worth the brackets: `<a>` is just text
        (_, Some(c)) if !c.is_control() && !mods.is_empty() => with_mods(Key::Char(c), mods),
        (Some(key), _) => with_mods(key, mods),
        _ => None,
    }
}

/// `key` held with `mods`, if a terminal could send it.
fn with_mods(key: Key, mods: Mods) -> Option<Key> {
    if mods.is_empty() {
        return Some(key);
    }
    match key {
        Key::Char(c) if mods == Mods::CTRL && c.is_ascii_alphabetic() => Some(Key::Ctrl(c.to_ascii_lowercase())),
        Key::Char(c) if mods == Mods::ALT => Some(Key::Alt(c)),
        // Alt-Shift-x is typed as Alt and X
        Key::Char(c) if mods == Mods::ALT | Mods::SHIFT && c.is_alphabetic() => {
            Some(Key::Alt(c.to_uppercase().next().unwrap_or(c)))
        }
        Key::Char(_) | Key::Ctrl(_) | Key::Alt(_) | Key::Esc | Key::BackTab | Key::Mod(..) => None,
        named => named_key(named).map(|n| Key::modified(n, mods)),
    }
}

fn named_key(key: Key) -> Option<NamedKey> {
    Some(match key {
        Key::Enter => NamedKey::Enter,
        Key::Tab => NamedKey::Tab,
        Key::Backspace => NamedKey::Backspace,
        Key::Delete => NamedKey::Delete,
        Key::Up => NamedKey::Up,
        Key::Down => NamedKey::Down,
        Key::Left => NamedKey::Left,
        Key::Right => NamedKey::Right,
        Key::Home => NamedKey::Home,
        Key::End => NamedKey::End,
        Key::PageUp => NamedKey::PageUp,
        Key::PageDown => NamedKey::PageDown,
        Key::F(n) => NamedKey::F(n),
        _ => return None,
    })
}

/// One key in vim's notation: a character on its own (`x`), or a name between `<` and `>`
/// with any of `C-`, `A-` (or `M-`) and `S-` before it, in any case: `<Esc>`, `<CR>`,
/// `<C-r>`, `<A-x>`, `<S-Tab>`, `<C-Up>`, `<F5>`, and `<lt>`, `<Space>` and `<Bar>` for
/// the characters that would read as something else.
///
/// ```
/// use tinsel::app::{Key, Mods, NamedKey};
///
/// assert_eq!("<C-a>".parse(), Ok(Key::Ctrl('a')));
/// assert_eq!("<esc>".parse(), Ok(Key::Esc));
/// assert_eq!("<A-x>".parse(), Ok(Key::Alt('x')));
/// assert_eq!("<C-S-Left>".parse(), Ok(Key::Mod(Mods::CTRL | Mods::SHIFT, NamedKey::Left)));
/// assert_eq!("é".parse(), Ok(Key::Char('é')));
/// assert!("<nonsense>".parse::<Key>().is_err());
/// ```
impl FromStr for Key {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Key, ParseKeyError> {
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return Ok(Key::Char(c));
        }
        s.strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .and_then(named)
            .ok_or_else(|| ParseKeyError(s.to_string()))
    }
}

/// The key in vim's notation, which [`FromStr`] reads back: `x`, `<lt>`, `<C-r>`, `<A-x>`,
/// `<CR>`, `<S-Tab>`, `<C-S-Left>`. A sequence of keys written one after another reads back
/// with [`Key::parse_sequence`].
///
/// ```
/// use tinsel::app::Key;
///
/// assert_eq!(Key::Ctrl('w').to_string(), "<C-w>");
/// assert_eq!(Key::Enter.to_string(), "<CR>");
/// let keys: String = Key::parse_sequence("dw<Esc>").iter().map(Key::to_string).collect();
/// assert_eq!(keys, "dw<Esc>");
/// ```
impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |key: NamedKey| match key {
            NamedKey::Enter => "CR".to_string(),
            NamedKey::Tab => "Tab".into(),
            NamedKey::Backspace => "BS".into(),
            NamedKey::Delete => "Del".into(),
            NamedKey::Up => "Up".into(),
            NamedKey::Down => "Down".into(),
            NamedKey::Left => "Left".into(),
            NamedKey::Right => "Right".into(),
            NamedKey::Home => "Home".into(),
            NamedKey::End => "End".into(),
            NamedKey::PageUp => "PageUp".into(),
            NamedKey::PageDown => "PageDown".into(),
            NamedKey::F(n) => format!("F{n}"),
        };
        match *self {
            Key::Char('<') => f.write_str("<lt>"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Ctrl(c) => write!(f, "<C-{c}>"),
            Key::Alt('>') => f.write_str("<A-gt>"),
            Key::Alt(c) => write!(f, "<A-{c}>"),
            Key::Esc => f.write_str("<Esc>"),
            Key::BackTab => f.write_str("<S-Tab>"),
            Key::Mod(mods, key) => {
                f.write_str("<")?;
                for (held, letter) in [(mods.ctrl(), "C-"), (mods.alt(), "A-"), (mods.shift(), "S-")] {
                    if held {
                        f.write_str(letter)?;
                    }
                }
                write!(f, "{}>", name(key))
            }
            key => match named_key(key) {
                Some(key) => write!(f, "<{}>", name(key)),
                None => write!(f, "{key:?}"),
            },
        }
    }
}

impl Key {
    /// Keys written one after another in vim's notation, as a mapping or a macro would be:
    /// `dw<Esc>` is d, w and Esc. A `<` that doesn't start a key's name is the character.
    ///
    /// ```
    /// use tinsel::app::Key;
    ///
    /// assert_eq!(Key::parse_sequence("i<C-r>"), [Key::Char('i'), Key::Ctrl('r')]);
    /// assert_eq!(Key::parse_sequence("a<b"), [Key::Char('a'), Key::Char('<'), Key::Char('b')]);
    /// ```
    pub fn parse_sequence(text: &str) -> Vec<Key> {
        let mut keys = Vec::new();
        let mut rest = text;
        while let Some(c) = rest.chars().next() {
            if c == '<' {
                let named = rest.find('>').and_then(|end| Some((named(&rest[1..end])?, end + 1)));
                if let Some((key, used)) = named {
                    keys.push(key);
                    rest = &rest[used..];
                    continue;
                }
            }
            keys.push(Key::Char(c));
            rest = &rest[c.len_utf8()..];
        }
        keys
    }

    /// The key as the symbols a Mac's menus use, for showing which key does what: `⌃A`,
    /// `⌥X`, `⇧⇥`, `⏎`, `⎋`, `↑`, and a character as itself (space as `␣`).
    ///
    /// ```
    /// use tinsel::app::{Key, Mods, NamedKey};
    ///
    /// assert_eq!(Key::Ctrl('a').symbol(), "⌃A");
    /// assert_eq!(Key::Esc.symbol(), "⎋");
    /// assert_eq!(Key::Mod(Mods::CTRL | Mods::SHIFT, NamedKey::Up).symbol(), "⌃⇧↑");
    /// ```
    pub fn symbol(self) -> String {
        let named = |key: NamedKey| match key {
            NamedKey::Enter => "⏎".to_string(),
            NamedKey::Tab => "⇥".into(),
            NamedKey::Backspace => "⌫".into(),
            NamedKey::Delete => "⌦".into(),
            NamedKey::Up => "↑".into(),
            NamedKey::Down => "↓".into(),
            NamedKey::Left => "←".into(),
            NamedKey::Right => "→".into(),
            NamedKey::Home => "↖".into(),
            NamedKey::End => "↘".into(),
            NamedKey::PageUp => "⇞".into(),
            NamedKey::PageDown => "⇟".into(),
            NamedKey::F(n) => format!("F{n}"),
        };
        match self {
            Key::Char(' ') => "␣".into(),
            Key::Char(c) => c.to_string(),
            Key::Ctrl(c) => format!("⌃{}", c.to_ascii_uppercase()),
            Key::Alt(c) => format!("⌥{}", c.to_uppercase()),
            Key::Esc => "⎋".into(),
            Key::BackTab => "⇧⇥".into(),
            Key::Mod(mods, key) => {
                let mut out = String::new();
                for (held, mark) in [(mods.ctrl(), '⌃'), (mods.alt(), '⌥'), (mods.shift(), '⇧')] {
                    if held {
                        out.push(mark);
                    }
                }
                out + &named(key)
            }
            key => named_key(key).map_or_else(|| format!("{key:?}"), named),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_reads_back_as_itself() {
        let keys = [
            Key::Char('a'),
            Key::Char('<'),
            Key::Char('>'),
            Key::Char(' '),
            Key::Char('-'),
            Key::Ctrl('z'),
            Key::Alt('x'),
            Key::Alt('X'),
            Key::Alt('<'),
            Key::Alt('>'),
            Key::Enter,
            Key::Tab,
            Key::BackTab,
            Key::Backspace,
            Key::Delete,
            Key::Esc,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::F(12),
            Key::Mod(Mods::CTRL, NamedKey::Left),
            Key::Mod(Mods::SHIFT | Mods::ALT, NamedKey::F(3)),
            Key::Mod(Mods::ALT, NamedKey::Enter),
        ];
        for key in keys {
            assert_eq!(key.to_string().parse::<Key>(), Ok(key), "{key}");
            assert_eq!(Key::parse_sequence(&key.to_string()), [key], "{key}");
        }
    }

    #[test]
    fn names_go_by_any_case_and_their_other_names() {
        for (text, key) in [
            ("<cr>", Key::Enter),
            ("<Return>", Key::Enter),
            ("<ESC>", Key::Esc),
            ("<s-tab>", Key::BackTab),
            ("<C-R>", Key::Ctrl('r')),
            ("<M-b>", Key::Alt('b')),
            ("<A-S-x>", Key::Alt('X')),
            ("<space>", Key::Char(' ')),
            ("<Bar>", Key::Char('|')),
            ("<f1>", Key::F(1)),
            ("<S-Up>", Key::Mod(Mods::SHIFT, NamedKey::Up)),
        ] {
            assert_eq!(text.parse::<Key>(), Ok(key), "{text}");
        }
        for bad in [
            "<a>", "", "ab", "<>", "<C-->", "<C-ab>", "<X-a>", "<F99>", "<C-Esc>", "<C-1>", "<Up",
        ] {
            let err = bad.parse::<Key>().unwrap_err();
            assert!(err.to_string().contains("isn't a key"), "{bad}");
        }
    }

    #[test]
    fn sequences_take_what_isnt_a_name_as_it_comes() {
        assert_eq!(
            Key::parse_sequence("dw<Esc><C-r><cr>a<b<lt>"),
            [
                Key::Char('d'),
                Key::Char('w'),
                Key::Esc,
                Key::Ctrl('r'),
                Key::Enter,
                Key::Char('a'),
                Key::Char('<'),
                Key::Char('b'),
                Key::Char('<'),
            ]
        );
        assert_eq!(Key::parse_sequence("<C-ab>").len(), 6);
        assert_eq!(Key::parse_sequence("<1>").len(), 3);
        assert_eq!(Key::parse_sequence("é<"), [Key::Char('é'), Key::Char('<')]);
        assert!(Key::parse_sequence("").is_empty());
    }

    #[test]
    fn symbols_for_every_kind_of_key() {
        let shown: Vec<String> = [
            Key::Char(' '),
            Key::Char('x'),
            Key::Alt('b'),
            Key::Enter,
            Key::Tab,
            Key::BackTab,
            Key::Backspace,
            Key::Delete,
            Key::Left,
            Key::Right,
            Key::Down,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::F(2),
            Key::Mod(Mods::ALT, NamedKey::Enter),
        ]
        .into_iter()
        .map(Key::symbol)
        .collect();
        assert_eq!(
            shown,
            [
                "␣", "x", "⌥B", "⏎", "⇥", "⇧⇥", "⌫", "⌦", "←", "→", "↓", "↖", "↘", "⇞", "⇟", "F2", "⌥⏎"
            ]
        );
    }
}

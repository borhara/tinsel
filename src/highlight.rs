//! Just enough syntax colouring for code excerpts: keywords, strings, numbers, comments, and
//! keys in config files. One line at a time, so a string or comment that spans lines is only
//! coloured on its first.

use crate::style::palette::{AMBER, BLUSH, MUTED, ROSE, SAGE, SOFT, TEXT};
use crate::style::{Rgb, mix, paint};

/// A language [`Code`](crate::Code) excerpts know how to colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// Rust.
    Rust,
    /// TOML.
    Toml,
    /// JSON.
    Json,
    /// YAML.
    Yaml,
    /// Shell scripts and commands.
    Shell,
    /// Python.
    Python,
    /// JavaScript and TypeScript.
    JavaScript,
}

impl Lang {
    /// The language with this name or file extension (`rust` or `rs`, `toml`, `json`,
    /// `yaml` or `yml`, `sh`/`bash`/`zsh`/`shell`, `python` or `py`, `js`/`ts`/`javascript`).
    pub fn from_name(name: &str) -> Option<Lang> {
        Some(match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Lang::Rust,
            "toml" => Lang::Toml,
            "json" | "jsonc" => Lang::Json,
            "yaml" | "yml" => Lang::Yaml,
            "sh" | "bash" | "zsh" | "shell" | "console" => Lang::Shell,
            "python" | "py" => Lang::Python,
            "js" | "ts" | "jsx" | "tsx" | "javascript" | "typescript" | "mjs" => Lang::JavaScript,
            _ => return None,
        })
    }

    /// The language a file is written in, from its extension.
    pub fn from_path(path: &str) -> Option<Lang> {
        Lang::from_name(path.rsplit_once('.')?.1)
    }

    fn comment(self) -> Option<&'static str> {
        match self {
            Lang::Rust | Lang::JavaScript => Some("//"),
            Lang::Toml | Lang::Yaml | Lang::Shell | Lang::Python => Some("#"),
            Lang::Json => None,
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn",
                "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
                "self", "Self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
            ],
            Lang::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else",
                "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "not", "or",
                "pass", "raise", "return", "try", "while", "with", "yield",
            ],
            Lang::JavaScript => &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "interface",
                "let",
                "new",
                "of",
                "return",
                "switch",
                "throw",
                "try",
                "type",
                "typeof",
                "var",
                "while",
                "yield",
            ],
            Lang::Shell => &[
                "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac", "function",
                "return", "export", "local",
            ],
            Lang::Toml | Lang::Json | Lang::Yaml => &[],
        }
    }

    fn constants(self) -> &'static [&'static str] {
        match self {
            Lang::Python => &["True", "False", "None"],
            Lang::JavaScript => &["true", "false", "null", "undefined"],
            _ => &["true", "false", "null", "None", "Some", "Ok", "Err"],
        }
    }
}

/// One line coloured as `lang`. Dimmed lines keep their colours but quieter, so a marked
/// line stands out from the ones around it.
pub(crate) fn line(src: &str, lang: Lang, dim: bool) -> String {
    let tone = |c: Rgb| if dim { mix(c, MUTED, 0.45) } else { c };
    let base = if dim { SOFT } else { TEXT };
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let take = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    // config files: a key at the start of the line, before `=` or `:`
    let key_end = config_key(&chars, lang);
    // shell: the first word is the command
    let mut command_pending = lang == Lang::Shell;

    while i < chars.len() {
        let c = chars[i];
        let rest: String = chars[i..].iter().collect();

        if let Some(end) = key_end.filter(|_| i == first_non_space(&chars)) {
            out += &paint(tone(BLUSH), &take(i, end));
            i = end;
            continue;
        }
        if lang.comment().is_some_and(|m| rest.starts_with(m)) {
            out += &paint(tone(MUTED), &rest);
            break;
        }
        if lang == Lang::Toml && c == '[' && i == first_non_space(&chars) {
            out += &paint(tone(ROSE), &rest);
            break;
        }
        if c == '"' || c == '`' || (c == '\'' && !is_lifetime(&chars, i, lang)) {
            let end = string_end(&chars, i);
            let text = take(i, end);
            // a JSON string followed by a colon is a key
            let is_key = lang == Lang::Json && chars[end..].iter().find(|c| !c.is_whitespace()) == Some(&':');
            out += &paint(tone(if is_key { BLUSH } else { SAGE }), &text);
            i = end;
            command_pending = false;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !is_word(chars[i - 1])) {
            let end = scan(&chars, i, |c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
            out += &paint(tone(AMBER), &take(i, end));
            i = end;
            continue;
        }
        if lang == Lang::Shell && c == '$' {
            let end = if chars.get(i + 1) == Some(&'{') {
                chars[i..]
                    .iter()
                    .position(|&c| c == '}')
                    .map_or(chars.len(), |p| i + p + 1)
            } else {
                scan(&chars, i + 1, is_word)
            };
            out += &paint(tone(BLUSH), &take(i, end));
            i = end;
            continue;
        }
        if is_word(c) || (lang == Lang::Shell && c == '-' && command_pending) {
            let end = scan(&chars, i, |c| is_word(c) || (lang == Lang::Shell && "-./".contains(c)));
            let word = take(i, end);
            let color = if command_pending {
                command_pending = false;
                ROSE
            } else if lang.keywords().contains(&word.as_str()) {
                ROSE
            } else if lang.constants().contains(&word.as_str()) {
                AMBER
            } else if lang == Lang::Shell && word.starts_with('-') {
                SOFT
            } else if lang == Lang::Rust && chars.get(end) == Some(&'!') {
                // macros
                ROSE
            } else if matches!(lang, Lang::Rust | Lang::JavaScript | Lang::Python)
                && word.starts_with(|c: char| c.is_ascii_uppercase())
            {
                // types and classes
                BLUSH
            } else {
                base
            };
            out += &paint(tone(color), &word);
            i = end;
            continue;
        }
        if lang == Lang::Shell && matches!(c, '|' | ';' | '&') {
            command_pending = true;
        }
        let end = scan(&chars, i + 1, |c| {
            !is_word(c) && !"\"'`#$/".contains(c) && !c.is_ascii_digit()
        });
        let end = end.max(i + 1);
        let punct = take(i, end);
        out += &paint(if c.is_whitespace() { base } else { tone(SOFT) }, &punct);
        i = end;
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn scan(chars: &[char], from: usize, keep: impl Fn(char) -> bool) -> usize {
    chars[from.min(chars.len())..]
        .iter()
        .position(|&c| !keep(c))
        .map_or(chars.len(), |p| from + p)
}

fn first_non_space(chars: &[char]) -> usize {
    chars.iter().position(|c| !c.is_whitespace()).unwrap_or(chars.len())
}

/// Where a string starting at `from` ends (just past its closing quote), allowing escapes.
fn string_end(chars: &[char], from: usize) -> usize {
    let quote = chars[from];
    let mut i = from + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// In Rust, `'a` is a lifetime, not the start of a character literal like `'a'`.
fn is_lifetime(chars: &[char], at: usize, lang: Lang) -> bool {
    lang == Lang::Rust && chars.get(at + 2) != Some(&'\'') && chars.get(at + 1) != Some(&'\\')
}

/// For TOML and YAML: where a `key =` or `key:` at the start of the line ends.
fn config_key(chars: &[char], lang: Lang) -> Option<usize> {
    let sep = match lang {
        Lang::Toml => '=',
        Lang::Yaml => ':',
        _ => return None,
    };
    let start = first_non_space(chars);
    // YAML list items can carry a key too: `- name: x`
    let start = if lang == Lang::Yaml && chars.get(start) == Some(&'-') {
        start + 1 + chars[start + 1..].iter().take_while(|c| c.is_whitespace()).count()
    } else {
        start
    };
    let end = scan(chars, start, |c| is_word(c) || "-.\"'".contains(c));
    let after = chars[end..].iter().position(|c| !c.is_whitespace()).map(|p| end + p);
    (end > start && after.is_some_and(|a| chars[a] == sep)).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::strip_ansi;

    #[test]
    fn colouring_never_changes_the_text() {
        crate::style::with_color_mode(crate::ColorMode::TrueColor, || {
            for (src, lang) in [
                (r#"let x: Vec<&'a str> = vec!["a\"b", 'c']; // hi"#, Lang::Rust),
                (r#"port = "80" # web"#, Lang::Toml),
                (r#"{"a": [1, true, null]}"#, Lang::Json),
                ("- name: ${HOME}/x", Lang::Yaml),
                (r#"cargo build --release | tee "$LOG" # go"#, Lang::Shell),
                ("def f(x): return None", Lang::Python),
                ("const a = `x${y}`;", Lang::JavaScript),
            ] {
                assert_eq!(strip_ansi(&line(src, lang, false)), src, "{lang:?}");
                assert_eq!(strip_ansi(&line(src, lang, true)), src, "{lang:?}");
            }
        });
    }

    #[test]
    fn finds_config_keys() {
        let chars: Vec<char> = "  memory = 512".chars().collect();
        assert_eq!(config_key(&chars, Lang::Toml), Some(8));
        let chars: Vec<char> = "- name: web".chars().collect();
        assert_eq!(config_key(&chars, Lang::Yaml), Some(6));
        let chars: Vec<char> = "just text".chars().collect();
        assert_eq!(config_key(&chars, Lang::Toml), None);
    }

    #[test]
    fn languages_come_from_paths() {
        assert_eq!(Lang::from_path("src/main.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("app.yml"), Some(Lang::Yaml));
        assert_eq!(Lang::from_path("Makefile"), None);
    }
}

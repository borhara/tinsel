//! JSON, indented and coloured: for showing API responses and config.

use std::fmt;

use crate::block::Block;
use crate::style::paint;
use crate::style::palette::{AMBER, BLUSH, MUTED, ROSE, SAGE};
use crate::term::content_width;
use crate::text;

/// Why [`json`] couldn't read its input, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    /// The line the problem is on, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    message: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at line {}, column {}", self.message, self.line, self.column)
    }
}

impl std::error::Error for JsonError {}

/// A parsed value, keeping strings and numbers exactly as written.
enum Value<'a> {
    Object(Vec<(&'a str, Value<'a>)>),
    Array(Vec<Value<'a>>),
    String(&'a str),
    Number(&'a str),
    Literal(&'a str),
}

/// JSON text indented and coloured: keys in blush, strings in sage, numbers in amber,
/// `true`/`false`/`null` in rose. Short arrays of plain values stay on one line. Key order
/// and the exact text of strings and numbers are kept.
///
/// ```
/// let pretty = tinsel::json(r#"{"name":"my-app","ports":[80,443],"tls":true}"#)?;
/// pretty.print();
/// assert_eq!(pretty.plain().lines().nth(2), Some(r#"    "ports": [80, 443],"#));
/// # Ok::<(), tinsel::JsonError>(())
/// ```
pub fn json(src: &str) -> Result<Block, JsonError> {
    let mut p = Parser { src, at: 0 };
    p.space();
    let value = p.value()?;
    p.space();
    if p.at < src.len() {
        return Err(p.error("unexpected text after the end"));
    }
    let mut out = Vec::new();
    let mut first = String::from("  ");
    draw(&value, 1, &mut first, &mut out);
    out.push(first);
    Ok(out.into())
}

struct Parser<'a> {
    src: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.at).copied()
    }

    fn space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn error(&self, message: &str) -> JsonError {
        let before = &self.src[..self.at.min(self.src.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        JsonError {
            line,
            column,
            message: message.into(),
        }
    }

    fn expect(&mut self, b: u8, what: &str) -> Result<(), JsonError> {
        self.space();
        if self.peek() == Some(b) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected {what}")))
        }
    }

    fn value(&mut self) -> Result<Value<'a>, JsonError> {
        self.space();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                let mut fields = Vec::new();
                self.space();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(Value::Object(fields));
                }
                loop {
                    self.space();
                    if self.peek() != Some(b'"') {
                        return Err(self.error("expected a key in quotes"));
                    }
                    let key = self.string()?;
                    self.expect(b':', "':' after the key")?;
                    fields.push((key, self.value()?));
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Value::Object(fields));
                        }
                        _ => return Err(self.error("expected ',' or '}'")),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Value::Array(items));
                        }
                        _ => return Err(self.error("expected ',' or ']'")),
                    }
                }
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self.peek().is_some_and(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
                    self.at += 1;
                }
                let n = &self.src[start..self.at];
                if n.parse::<f64>().is_err() {
                    self.at = start;
                    return Err(self.error("this isn't a number"));
                }
                Ok(Value::Number(n))
            }
            Some(_) => {
                for word in ["true", "false", "null"] {
                    if self.src[self.at..].starts_with(word) {
                        self.at += word.len();
                        return Ok(Value::Literal(word));
                    }
                }
                Err(self.error("expected a value"))
            }
            None => Err(self.error("the text ended too soon")),
        }
    }

    /// A string, quotes included, exactly as written.
    fn string(&mut self) -> Result<&'a str, JsonError> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(&self.src[start..self.at]);
                }
                Some(b'\\') => self.at += 2,
                Some(_) => self.at += 1,
                None => {
                    self.at = start;
                    return Err(self.error("this string never ends"));
                }
            }
        }
    }
}

/// Short arrays of plain values on one line, if they fit.
fn inline(value: &Value) -> Option<String> {
    let Value::Array(items) = value else { return None };
    let parts: Option<Vec<String>> = items
        .iter()
        .map(|v| match v {
            Value::String(s) => Some(paint(SAGE, s)),
            Value::Number(n) => Some(paint(AMBER, n)),
            Value::Literal(l) => Some(paint(ROSE, l)),
            _ => None,
        })
        .collect();
    let line = format!(
        "{}{}{}",
        paint(MUTED, "["),
        parts?.join(&paint(MUTED, ", ")),
        paint(MUTED, "]")
    );
    (text::width(&line) <= 40).then_some(line)
}

/// Draw `value` onto the end of `line`, pushing finished lines to `out`.
fn draw(value: &Value, depth: usize, line: &mut String, out: &mut Vec<String>) {
    let indent = |d: usize| format!("  {}", "  ".repeat(d));
    match value {
        Value::String(s) => *line += &paint(SAGE, s),
        Value::Number(n) => *line += &paint(AMBER, n),
        Value::Literal(l) => *line += &paint(ROSE, l),
        Value::Array(items) if items.is_empty() => *line += &paint(MUTED, "[]"),
        Value::Object(fields) if fields.is_empty() => *line += &paint(MUTED, "{}"),
        Value::Array(_) if inline(value).is_some_and(|l| text::width(line) + text::width(&l) < content_width()) => {
            *line += &inline(value).unwrap_or_default();
        }
        Value::Array(items) => {
            *line += &paint(MUTED, "[");
            for (i, item) in items.iter().enumerate() {
                out.push(std::mem::replace(line, indent(depth)));
                draw(item, depth + 1, line, out);
                if i + 1 < items.len() {
                    *line += &paint(MUTED, ",");
                }
            }
            out.push(std::mem::replace(line, indent(depth - 1)));
            *line += &paint(MUTED, "]");
        }
        Value::Object(fields) => {
            *line += &paint(MUTED, "{");
            for (i, (key, v)) in fields.iter().enumerate() {
                out.push(std::mem::replace(line, indent(depth)));
                *line += &format!("{}{} ", paint(BLUSH, key), paint(MUTED, ":"));
                draw(v, depth + 1, line, out);
                if i + 1 < fields.len() {
                    *line += &paint(MUTED, ",");
                }
            }
            out.push(std::mem::replace(line, indent(depth - 1)));
            *line += &paint(MUTED, "}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_nested_values() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = json(r#"{"a":{"b":[1,{"c":null}]},"e":[],"s":"x\"y"}"#).unwrap();
        assert_eq!(
            out.plain(),
            [
                "  {",
                r#"    "a": {"#,
                r#"      "b": ["#,
                "        1,",
                "        {",
                r#"          "c": null"#,
                "        }",
                "      ]",
                "    },",
                r#"    "e": [],"#,
                r#"    "s": "x\"y""#,
                "  }",
            ]
            .join("\n")
        );
    }

    #[test]
    fn says_where_it_went_wrong() {
        let e = json("{\n  \"a\": 1,\n  \"b\" 2\n}").unwrap_err();
        assert_eq!((e.line, e.column), (3, 7));
        assert_eq!(e.to_string(), "expected ':' after the key at line 3, column 7");
        assert!(json("[1, 2").is_err());
        assert!(json("\"open").is_err());
        assert!(json("1 2").is_err());
    }

    #[test]
    fn empty_objects_and_short_arrays_stay_on_one_line() {
        crate::set_color_mode(crate::ColorMode::None);
        let out = json(r#"{"o":{},"a":["x",1,true]}"#).unwrap().plain();
        assert!(
            out.contains(r#""o": {}"#) && out.contains(r#""a": ["x", 1, true]"#),
            "{out}"
        );
    }

    #[test]
    fn long_or_nested_arrays_get_a_line_each() {
        crate::set_color_mode(crate::ColorMode::None);
        let long = format!(r#"["{}", "{}"]"#, "a".repeat(30), "b".repeat(30));
        assert_eq!(json(&long).unwrap().len(), 4);
        assert_eq!(json("[[1]]").unwrap().len(), 3);
    }

    #[test]
    fn explains_each_kind_of_mistake() {
        let msg = |s: &str| json(s).unwrap_err().to_string();
        assert!(msg("{1: 2}").starts_with("expected a key in quotes"));
        assert!(msg(r#"{"a": 1 "b"}"#).starts_with("expected ',' or '}'"));
        assert!(msg("[1-2]").starts_with("this isn't a number"));
        assert!(msg("[nope]").starts_with("expected a value"));
        assert!(msg("[1,").starts_with("the text ended too soon"));
    }
}

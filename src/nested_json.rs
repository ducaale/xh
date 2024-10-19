//! Insert value into JSON at an arbitrary location specified by a JSON path.
//!
//! Where JSON path is a string that satisfies the following syntax grammar:
//! ```
//!   start: root_path path*
//!   root_path: (literal | index_path | append_path)
//!   literal: TEXT | NUMBER
//!
//!   path: key_path | index_path | append_path
//!   key_path: LEFT_BRACKET TEXT RIGHT_BRACKET
//!   index_path: LEFT_BRACKET NUMBER RIGHT_BRACKET
//!   append_path: LEFT_BRACKET RIGHT_BRACKET
//! ```
//!
//! Additionally, a backslash character can be used to:
//! - Escape characters with especial meaning such as `\`, `[` and `]`.
//! - Treat numbers as a key rather than as an index.

use std::{fmt, mem};

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

#[derive(Debug, Clone)]
enum Token {
    LeftBracket(usize),
    RightBracket(usize),
    Text(String, (usize, usize)),
    Number(usize, (usize, usize)),
}

impl Token {
    fn literal(json_path: &str, start: Option<usize>, end: Option<usize>) -> Token {
        const SPECIAL_CHARS: &str = "=@:;[]\\";
        let start = start.map_or(0, |s| s + 1);
        let end = end.unwrap_or(json_path.len());
        let span = (start, end);

        if start == 0 {
            // The first token is never interpreted as a number and therefore
            // the number escaping rule doesn't apply to it.
            Token::Text(unescape(&json_path[start..end], SPECIAL_CHARS), span)
        } else {
            let literal = &json_path[start..end];
            if literal.starts_with('\\') && literal[1..].parse::<usize>().is_ok() {
                Token::Text(literal[1..].to_string(), span)
            } else if let Ok(n) = literal.parse::<usize>() {
                Token::Number(n, span)
            } else {
                Token::Text(unescape(literal, SPECIAL_CHARS), span)
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Token::Text(_, (start, end)) => start == end,
            _ => false,
        }
    }
}

fn tokenize(json_path: &str) -> impl IntoIterator<Item = Token> {
    let mut tokens = vec![];
    let mut escaped = false;
    let mut last_delim_pos = None;

    for (i, ch) in json_path.char_indices() {
        if ch == '\\' {
            escaped = !escaped;
        } else if ch == '[' && !escaped {
            tokens.push(Token::literal(json_path, last_delim_pos, Some(i)));
            tokens.push(Token::LeftBracket(i));
            last_delim_pos = Some(i);
            escaped = false;
        } else if ch == ']' && !escaped {
            tokens.push(Token::literal(json_path, last_delim_pos, Some(i)));
            tokens.push(Token::RightBracket(i));
            last_delim_pos = Some(i);
            escaped = false;
        } else {
            escaped = false;
        }
    }
    tokens.push(Token::literal(json_path, last_delim_pos, None));

    tokens.into_iter().filter(|t| !t.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAction {
    Key(String, (usize, usize)),
    Index(usize, (usize, usize)),
    Append((usize, usize)),
}

pub fn parse_path(json_path: &str) -> Result<Vec<PathAction>> {
    use PathAction::*;
    use Token::*;

    let mut path = vec![];
    let mut tokens_iter = tokenize(json_path).into_iter();

    match tokens_iter.next() {
        Some(LeftBracket(start)) => match tokens_iter.next() {
            Some(Number(index, _)) => {
                if let Some(RightBracket(end)) = tokens_iter.next() {
                    path.push(Index(index, (start, end + 1)));
                } else {
                    return Err(syntax_error("']'", start + 1, json_path));
                }
            }
            Some(RightBracket(end)) => path.push(Append((start, end + 1))),
            Some(Text(..) | LeftBracket(..)) | None => {
                return Err(syntax_error("number or ']'", start + 1, json_path))
            }
        },
        Some(Text(key, span)) => path.push(Key(key, span)),
        Some(Number(..) | RightBracket(..)) | None => {
            return Err(syntax_error("text or '['", 0, json_path))
        }
    }

    while let Some(token) = tokens_iter.next() {
        let start = match token {
            LeftBracket(start) => start,
            RightBracket(start) | Text(_, (start, _)) | Number(_, (start, _)) => {
                return Err(syntax_error("'['", start, json_path));
            }
        };

        match tokens_iter.next() {
            Some(Number(index, span)) => {
                if let Some(RightBracket(end)) = tokens_iter.next() {
                    path.push(Index(index, (start, end + 1)));
                } else {
                    return Err(syntax_error("']'", span.1, json_path));
                }
            }
            Some(Text(key, span)) => {
                if let Some(RightBracket(end)) = tokens_iter.next() {
                    path.push(Key(key, (start, end + 1)));
                } else {
                    return Err(syntax_error("']'", span.1, json_path));
                }
            }
            Some(RightBracket(end)) => path.push(Append((start, end + 1))),
            Some(LeftBracket(..)) | None => {
                return Err(syntax_error("text, number or ']'", start + 1, json_path))
            }
        }
    }

    Ok(path)
}

pub fn insert(
    root: Option<Value>,
    path: &[PathAction],
    value: Value,
) -> std::result::Result<Value, Box<TypeError>> {
    assert!(!path.is_empty(), "path should not be empty");

    Ok(match root {
        Some(Value::Object(mut obj)) => {
            let key = match &path[0] {
                PathAction::Key(v, ..) => v.to_string(),
                path_component @ (PathAction::Index(..) | PathAction::Append(..)) => {
                    return Err(Box::new(TypeError::new(
                        Value::Object(obj),
                        path_component.clone(),
                    )))
                }
            };
            if path.len() == 1 {
                obj.insert(key, value);
            } else {
                let temp = obj.remove(&key);
                let value = insert(temp, &path[1..], value)?;
                obj.insert(key, value);
            };
            Value::Object(obj)
        }
        Some(Value::Array(mut arr)) => {
            let index = match &path[0] {
                path_component @ PathAction::Key(..) => {
                    return Err(Box::new(TypeError::new(
                        Value::Array(arr),
                        path_component.clone(),
                    )))
                }
                PathAction::Index(v, ..) => *v,
                PathAction::Append(..) => arr.len(),
            };
            if path.len() == 1 {
                arr_insert(&mut arr, index, value);
            } else {
                let temp = remove_from_arr(&mut arr, index);
                let value = insert(temp, &path[1..], value)?;
                arr_insert(&mut arr, index, value);
            };
            Value::Array(arr)
        }
        Some(root) => {
            return Err(Box::new(TypeError::new(root, path[0].clone())));
        }
        None => match path[0] {
            PathAction::Key(..) => insert(Some(Value::Object(Map::new())), path, value)?,
            PathAction::Index(..) | PathAction::Append(..) => {
                insert(Some(Value::Array(vec![])), path, value)?
            }
        },
    })
}

/// Inserts value into array at any index and pads empty slots
/// with Value::null if needed
fn arr_insert(arr: &mut Vec<Value>, index: usize, value: Value) {
    while index >= arr.len() {
        arr.push(Value::Null);
    }
    arr[index] = value;
}

/// Removes an element from array and replace it with `Value::Null`.
fn remove_from_arr(arr: &mut [Value], index: usize) -> Option<Value> {
    if index < arr.len() {
        Some(mem::replace(&mut arr[index], Value::Null))
    } else {
        None
    }
}

fn syntax_error(expected: &'static str, pos: usize, json_path: &str) -> anyhow::Error {
    anyhow!(
        "expected {}\n\n{}",
        expected,
        highlight_error(json_path, pos, pos + 1)
    )
}

fn highlight_error(text: &str, start: usize, mut end: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    // Apply right-padding so outside of the text could be highlighted
    let text = format!("{:<min_width$}", text, min_width = end);
    // Ensure end doesn't fall on non-char boundary
    while !text.is_char_boundary(end) && end < text.len() {
        end += 1;
    }
    format!(
        "  {}\n  {}{}",
        text,
        " ".repeat(text[0..start].width()),
        "^".repeat(text[start..end].width())
    )
}

#[derive(Debug, Clone)]
pub struct TypeError {
    root: Value,
    path_component: PathAction,
    json_path: Option<String>,
}

impl TypeError {
    fn new(root: Value, path_component: PathAction) -> Self {
        TypeError {
            root,
            path_component,
            json_path: None,
        }
    }

    pub fn with_json_path(mut self, json_path: String) -> TypeError {
        self.json_path = Some(json_path);
        self
    }
}

impl std::error::Error for TypeError {}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_type = match self.root {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        let (access_type, expected_root_type, (start, end)) = match self.path_component {
            PathAction::Append(pos) => ("append", "array", pos),
            PathAction::Index(_, pos) => ("index", "array", pos),
            PathAction::Key(_, pos) => ("key", "object", pos),
        };

        if let Some(json_path) = &self.json_path {
            write!(
                f,
                "Can't perform '{}' based access on '{}' which has a type of '{}' but this operation requires a type of '{}'.\n\n{}",
                access_type,
                &json_path[..start],
                root_type,
                expected_root_type,
                highlight_error(json_path, start, end)
            )
        } else {
            write!(
                f,
                "Can't perform '{}' based access on '{}'",
                access_type, root_type
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn deeply_nested_object() {
        let root = insert(None, &parse_path("foo[bar][baz]").unwrap(), 5.into());
        assert_eq!(root.unwrap(), json!({"foo": {"bar": {"baz": 5}}}));
    }

    #[test]
    fn deeply_nested_array() {
        let root = insert(None, &parse_path("[0][0][1]").unwrap(), 5.into());
        assert_eq!(root.unwrap(), json!([[[null, 5]]]));
    }

    #[test]
    fn can_override_value() {
        let root = insert(None, &parse_path("foo[x]").unwrap(), 5.into());
        assert_eq!(root.clone().unwrap(), json!({"foo": {"x": 5}}));

        let root = insert(
            root.unwrap().into(),
            &parse_path("foo[y]").unwrap(),
            true.into(),
        );
        assert_eq!(root.clone().unwrap(), json!({"foo": {"x": 5, "y": true}}));

        let root = insert(
            root.unwrap().into(),
            &parse_path("foo[x]").unwrap(),
            6.into(),
        );
        assert_eq!(root.unwrap(), json!({"foo": {"x": 6, "y": true}}));
    }

    #[test]
    fn type_clashes() {
        // object array clash
        let root = insert(None, &parse_path("foo").unwrap(), 5.into());
        let root = insert(root.unwrap().into(), &parse_path("[0]").unwrap(), 5.into());
        assert!(root.is_err());

        // array object clash
        let root = insert(None, &parse_path("[0]").unwrap(), 5.into());
        let root = insert(root.unwrap().into(), &parse_path("foo").unwrap(), 5.into());
        assert!(root.is_err());

        // number object clash
        let root = insert(None, &parse_path("foo").unwrap(), 5.into());
        let root = insert(
            root.unwrap().into(),
            &parse_path("foo[x]").unwrap(),
            5.into(),
        );
        assert!(root.is_err());

        // number array clash
        let root = insert(None, &parse_path("foo").unwrap(), 5.into());
        let root = insert(
            root.unwrap().into(),
            &parse_path("foo[0]").unwrap(),
            5.into(),
        );
        assert!(root.is_err());
    }

    #[test]
    fn json_path_parser() {
        use PathAction::*;

        assert_eq!(
            parse_path(r"foo\[x\][]").unwrap(),
            [Key(r"foo[x]".into(), (0, 8)), Append((8, 10))]
        );
        assert_eq!(
            parse_path(r"foo\\[x]").unwrap(),
            [Key(r"foo\".into(), (0, 5)), Key("x".into(), (5, 8))]
        );
        assert_eq!(
            parse_path(r"foo[ba\[ar][9]").unwrap(),
            [
                Key("foo".into(), (0, 3)),
                Key("ba[ar".into(), (3, 11)),
                Index(9, (11, 14))
            ]
        );
        assert_eq!(
            parse_path(r"[0][foo]").unwrap(),
            [Index(0, (0, 3)), Key("foo".into(), (3, 8))]
        );
        assert_eq!(
            parse_path(r"[][foo]").unwrap(),
            [Append((0, 2)), Key("foo".into(), (2, 7))]
        );
        assert_eq!(
            parse_path(r"foo[0]").unwrap(),
            [Key("foo".into(), (0, 3)), Index(0, (3, 6))]
        );
        assert_eq!(
            parse_path(r"foo[\0]").unwrap(),
            [Key("foo".into(), (0, 3)), Key("0".into(), (3, 7))]
        );
        assert_eq!(
            parse_path(r"foo[\\0]").unwrap(),
            [Key("foo".into(), (0, 3)), Key(r"\0".into(), (3, 8)),]
        );
        // HTTPie currently escapes numbers regardless of whether they are between
        // square brackets or not. See https://github.com/httpie/httpie/issues/1408
        //
        // $ http --offline --pretty=none --print=B : \\0[\\5]=x
        // {"0":{"5": "x"}}
        // $ xh --offline --pretty=none --print=B : \\0[\\5]=x
        // {"\\0": {"5": "x"}}
        assert_eq!(parse_path(r"\5").unwrap(), [Key(r"\5".into(), (0, 2))]);
        assert_eq!(
            parse_path(r"5[x]").unwrap(),
            [Key("5".into(), (0, 1)), Key("x".into(), (1, 4))]
        );

        assert!(parse_path(r"[y][5]").is_err());
        assert!(parse_path(r"x[y]h[z]").is_err());
        assert!(parse_path(r"x[y]h").is_err());
        assert!(parse_path(r"[x][y]h").is_err());
        assert!(parse_path("foo[😀]x").is_err());
        assert!(parse_path(r"foo[bar]\[baz]").is_err());
        assert!(parse_path(r"foo\[bar][baz]").is_err());

        // shouldn't panic when highlighting a key with unicode chars
        assert!(parse_path("[😀").is_err());
        assert!(parse_path("[][😀").is_err());
    }
}

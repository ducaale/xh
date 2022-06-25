use std::{fmt, mem};

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAction {
    Key(String, (usize, usize)),
    Index(usize, (usize, usize)),
    Append((usize, usize)),
}

/// Parses a JSON path.
///
/// A JSON path is a string that satisfies the following syntax grammar:
/// ```
///   start: root_path path*
///   root_path: (literal | index_path | append_path)
///   literal: TEXT | NUMBER
///
///   path: key_path | index_path | append_path
///   key_path: LEFT_BRACKET TEXT RIGHT_BRACKET
///   index_path: LEFT_BRACKET NUMBER RIGHT_BRACKET
///   append_path: LEFT_BRACKET RIGHT_BRACKET
/// ```
///
/// Additionally, a backslash character can be used to:
/// - Escape characters with especial meaning such as `\`, `[` and `]`.
/// - Treat numbers as a key rather than as an index.
pub fn parse_path(raw_json_path: &str) -> Result<Vec<PathAction>> {
    use PathAction::*;
    const SPECIAL_CHARS: &str = "=@:;[]\\";
    let mut delims = vec![];
    let mut backslashes = 0;

    for (i, ch) in raw_json_path.char_indices() {
        if ch == '\\' {
            backslashes += 1;
        } else {
            if (ch == '[' || ch == ']') && backslashes % 2 == 0 {
                delims.push((i, ch));
            }
            backslashes = 0;
        }
    }

    if delims.is_empty() {
        return Ok(vec![Key(
            unescape(raw_json_path, SPECIAL_CHARS),
            (0, raw_json_path.len()),
        )]);
    }
    if delims.len() % 2 != 0 {
        return Err(anyhow!("{:?} unbalanced number of brackets", raw_json_path));
    }
    let mut prev_closing_bracket = None;
    for pair in delims.chunks_exact(2) {
        if let Some(prev_closing_bracket) = prev_closing_bracket {
            let current_opening_bracket = pair[0].0;
            if current_opening_bracket - prev_closing_bracket > 1 {
                return Err(anyhow!(
                    "{:?} unexpected string after closing bracket at index {}",
                    raw_json_path,
                    prev_closing_bracket + 1
                ));
            }
        }
        if pair[0].1 == ']' {
            return Err(anyhow!(
                "{:?} unexpected closing bracket at index {}",
                raw_json_path,
                pair[0].0
            ));
        }
        if pair[1].1 == '[' {
            return Err(anyhow!(
                "{:?} unexpected opening bracket at index {}",
                raw_json_path,
                pair[1].0
            ));
        }
        prev_closing_bracket = Some(pair[1].0);
    }

    if let Some(last_closing_bracket) = prev_closing_bracket {
        if last_closing_bracket != raw_json_path.len() - 1 {
            return Err(anyhow!(
                "{:?} unexpected string after closing bracket at index {}",
                raw_json_path,
                last_closing_bracket + 1
            ));
        }
    }

    let mut json_path = vec![];

    // handle any literals found before the first `[`
    if delims[0].0 > 0 {
        // raw_json_path starts with a literal e.g `foo[x]`, `foo[5]` or `foo[]`
        json_path.push(Key(
            unescape(&raw_json_path[0..delims[0].0], SPECIAL_CHARS),
            (0, delims[0].0),
        ));
    } else {
        let key = &raw_json_path[delims[0].0 + 1..delims[1].0];
        if !key.is_empty() && key.parse::<usize>().is_err() {
            // raw_json_path starts with `[string]`
            return Err(anyhow!(
                "{:?} Unexpected string after opening bracket at index {}",
                raw_json_path,
                1
            ));
        }
    }

    for pair in delims.chunks_exact(2) {
        let (start, end) = (pair[0].0, pair[1].0 + 1);
        let path_component = &raw_json_path[(start + 1)..(end - 1)];

        if let Ok(index) = path_component.parse::<usize>() {
            json_path.push(Index(index, (start, end)));
        } else if path_component.is_empty() {
            json_path.push(Append((start, end)));
        } else if path_component.starts_with('\\') && path_component[1..].parse::<usize>().is_ok() {
            // No need to escape `path_component[1..]` as it was successfully parsed as a number before.
            json_path.push(Key(path_component[1..].to_string(), (start, end)));
        } else {
            json_path.push(Key(unescape(path_component, SPECIAL_CHARS), (start, end)));
        }
    }

    Ok(json_path)
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

fn highlight_error(text: &str, start: usize, end: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    format!(
        "  {}\n  {}{}",
        text,
        " ".repeat(text[0..start].width()),
        "^".repeat(text[start..end].width())
    )
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

pub fn insert(
    root: Option<Value>,
    path: &[PathAction],
    value: Value,
) -> std::result::Result<Value, TypeError> {
    assert!(!path.is_empty(), "path should not be empty");

    Ok(match root {
        Some(Value::Object(mut obj)) => {
            let key = match &path[0] {
                PathAction::Key(v, ..) => v.to_string(),
                path_component @ (PathAction::Index(..) | PathAction::Append(..)) => {
                    return Err(TypeError::new(Value::Object(obj), path_component.clone()))
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
                    return Err(TypeError::new(Value::Array(arr), path_component.clone()))
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
            return Err(TypeError::new(root, path[0].clone()));
        }
        None => match path[0] {
            PathAction::Key(..) => insert(Some(Value::Object(Map::new())), path, value)?,
            PathAction::Index(..) | PathAction::Append(..) => {
                insert(Some(Value::Array(vec![])), path, value)?
            }
        },
    })
}

/// Inserts value into array at any index greater or equal to 0.
fn arr_insert(arr: &mut Vec<Value>, index: usize, value: Value) {
    while index >= arr.len() {
        arr.push(Value::Null);
    }
    arr[index] = value;
}

/// Removes an element from array and replace it with `Value::Null`.
fn remove_from_arr(arr: &mut Vec<Value>, index: usize) -> Option<Value> {
    if index < arr.len() {
        Some(mem::replace(&mut arr[index], Value::Null))
    } else {
        None
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
            &[Key(r"foo[x]".into(), (0, 8)), Append((8, 10))]
        );
        assert_eq!(
            parse_path(r"foo\\[x]").unwrap(),
            &[Key(r"foo\".into(), (0, 5)), Key("x".into(), (5, 8))]
        );
        assert_eq!(
            parse_path(r"foo[ba\[ar][9]").unwrap(),
            &[
                Key("foo".into(), (0, 3)),
                Key("ba[ar".into(), (3, 11)),
                Index(9, (11, 14))
            ]
        );
        assert_eq!(
            parse_path(r"[0][foo]").unwrap(),
            &[Index(0, (0, 3)), Key("foo".into(), (3, 8))]
        );
        assert_eq!(
            parse_path(r"[][foo]").unwrap(),
            &[Append((0, 2)), Key("foo".into(), (2, 7))]
        );
        assert_eq!(
            parse_path(r"foo[0]").unwrap(),
            &[Key("foo".into(), (0, 3)), Index(0, (3, 6))]
        );
        assert_eq!(
            parse_path(r"foo[\0]").unwrap(),
            &[Key("foo".into(), (0, 3)), Key("0".into(), (3, 7))]
        );
        assert_eq!(
            parse_path(r"foo[\\0]").unwrap(),
            &[Key("foo".into(), (0, 3)), Key(r"\0".into(), (3, 8)),]
        );
        // HTTPie currently escapes numbers regardless of whether they are between
        // square brackets or not. See https://github.com/httpie/httpie/issues/1408
        //
        // $ http --offline --pretty=none --print=B : \\0[\\5]=x
        // {"0":{"5": "x"}}
        // $ xh --offline --pretty=none --print=B : \\0[\\5]=x
        // {"\\0": {"5": "x"}}
        assert_eq!(parse_path(r"\5").unwrap(), &[Key(r"\5".into(), (0, 2))]);
        assert_eq!(
            parse_path(r"5[x]").unwrap(),
            &[Key("5".into(), (0, 1)), Key("x".into(), (1, 4))]
        );

        assert!(parse_path(r"[y][5]").is_err());
        assert!(parse_path(r"x[y]h[z]").is_err());
        assert!(parse_path(r"x[y]h").is_err());
        assert!(parse_path(r"[x][y]h").is_err());
        assert!(parse_path("foo[😀]x").is_err());
        assert!(parse_path(r"foo[bar]\[baz]").is_err());
        assert!(parse_path(r"foo\[bar][baz]").is_err());
    }
}

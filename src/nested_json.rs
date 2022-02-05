use std::{fmt, mem};

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathComponent {
    Key(String, Option<(usize, usize)>),
    Index(Option<usize>, (usize, usize)),
}

/// Parse a JSON path.
///
/// A valid JSON path is `literal([index])*` where `index` is either a `text`, `number` or `empty`.
///
/// Just like any `request_item`, special characters e.g `[` can be escaped with
/// a backslash character.
///
/// **TODO**: mention escaped numbers
pub fn parse_path(raw_json_path: &str) -> Result<Vec<PathComponent>> {
    use PathComponent::*;
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
        return Ok(vec![Key(unescape(raw_json_path, SPECIAL_CHARS), None)]);
    }
    if delims.len() % 2 != 0 {
        return Err(anyhow!("unbalanced number of brackets {:?}", raw_json_path));
    }
    let mut prev_closing_bracket = None;
    for pair in delims.chunks_exact(2) {
        if let Some(prev_closing_bracket) = prev_closing_bracket {
            let current_opening_bracket = pair[0].0;
            if current_opening_bracket - prev_closing_bracket > 1 {
                return Err(anyhow!(
                    "unexpected string after closing bracket at index {}",
                    prev_closing_bracket + 1
                ));
            }
        }
        if pair[0].1 == ']' {
            return Err(anyhow!("unexpected closing bracket at index {}", pair[0].0));
        }
        if pair[1].1 == '[' {
            return Err(anyhow!("unexpected opening bracket at index {}", pair[1].0));
        }
        prev_closing_bracket = Some(pair[1].0);
    }

    if let Some(last_closing_bracket) = prev_closing_bracket {
        if last_closing_bracket != raw_json_path.bytes().count() - 1 {
            return Err(anyhow!(
                "unexpected string after closing bracket at index {}",
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
            None,
        ));
    } else {
        let key = &raw_json_path[delims[0].0 + 1..delims[1].0];
        if !key.is_empty() && key.parse::<usize>().is_err() {
            // raw_json_path starts with `[string]`
            // TODO: throw error instead. See https://github.com/httpie/httpie/pull/1292
            json_path.push(Key("".to_string(), None));
        }
    }

    for pair in delims.chunks_exact(2) {
        let (start, end) = (pair[0].0, pair[1].0);
        let path_component = &raw_json_path[(start + 1)..end];

        if let Ok(index) = path_component.parse::<usize>() {
            json_path.push(Index(Some(index), (start, end)));
        } else if path_component.is_empty() {
            json_path.push(Index(None, (start, end)));
        } else if path_component.starts_with('\\') && path_component[1..].parse::<usize>().is_ok() {
            // No need to escape `path_component[1..]` as it was successfully parsed as a number before.
            json_path.push(Key(path_component[1..].to_string(), Some((start, end))));
        } else {
            json_path.push(Key(
                unescape(path_component, SPECIAL_CHARS),
                Some((start, end)),
            ));
        }
    }

    Ok(json_path)
}

#[derive(Debug)]
pub struct TypeError {
    root: Value,
    path_component: PathComponent,
    json_path: Option<String>,
}

impl TypeError {
    fn new(root: Value, path_component: PathComponent) -> Self {
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
            PathComponent::Index(None, delims_pos) => ("append", "array", delims_pos),
            PathComponent::Index(Some(_), delims_pos) => ("index", "array", delims_pos),
            PathComponent::Key(_, Some(x)) => ("key", "object", x),
            PathComponent::Key(_, None) => unreachable!(),
        };

        if let Some(json_path) = &self.json_path {
            write!(
                f,
                "Can't perform '{}' based access on '{}' which has a type of '{}' but this operation requires a type of '{}'.\n{}",
                access_type,
                &json_path[..start],
                root_type,
                expected_root_type,
                format_args!("{}\n{}{}", json_path, " ".repeat(start), "^".repeat(end - start + 1))
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

impl std::error::Error for TypeError {}

// TODO: add comment here
// TOOD: come up with a better name for this function
pub fn set_value(
    root: Option<Value>,
    path: &[PathComponent],
    value: Value,
) -> std::result::Result<Value, TypeError> {
    debug_assert!(!path.is_empty(), "path should not be empty");
    Ok(match root {
        Some(Value::Object(mut obj)) => {
            let key = match &path[0] {
                PathComponent::Key(v, ..) => v.to_string(),
                path_component @ PathComponent::Index(..) => {
                    return Err(TypeError::new(Value::Object(obj), path_component.clone()))
                }
            };
            if path.len() == 1 {
                obj.insert(key, value);
            } else {
                let temp = obj.remove(&key);
                let value = set_value(temp, &path[1..], value)?;
                obj.insert(key, value);
            };
            Value::Object(obj)
        }
        Some(Value::Array(mut arr)) => {
            let index = match &path[0] {
                path_component @ PathComponent::Key(..) => {
                    return Err(TypeError::new(Value::Array(arr), path_component.clone()))
                }
                PathComponent::Index(Some(v), ..) => *v,
                PathComponent::Index(None, ..) => arr.len(),
            };
            if path.len() == 1 {
                arr_insert(&mut arr, index, value);
            } else {
                let temp = remove_from_arr(&mut arr, index);
                let value = set_value(temp, &path[1..], value)?;
                arr_insert(&mut arr, index, value);
            };
            Value::Array(arr)
        }
        Some(root) => {
            if path.len() == 1 {
                value
            } else {
                return Err(TypeError::new(root, path[0].clone()));
            }
        }
        None => match path[0] {
            PathComponent::Key(..) => set_value(Some(Value::Object(Map::new())), path, value)?,
            PathComponent::Index(..) => set_value(Some(Value::Array(vec![])), path, value)?,
        },
    })
}

/// Insert value into array at any index ≥ 0
fn arr_insert(arr: &mut Vec<Value>, index: usize, value: Value) {
    while index >= arr.len() {
        arr.push(Value::Null);
    }
    arr[index] = value;
}

/// Remove an element from array and replace it with `Value::Null`.
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
        let root = set_value(None, &parse_path("foo[bar][baz]").unwrap(), 5.into());
        assert_eq!(root.unwrap(), json!({"foo": {"bar": {"baz": 5}}}));
    }

    #[test]
    fn deeply_nested_array() {
        let root = set_value(None, &parse_path("[0][0][1]").unwrap(), 5.into());
        assert_eq!(root.unwrap(), json!([[[null, 5]]]));
    }

    // TODO: add tests for type clash errors

    #[test]
    fn json_path_parser() {
        use PathComponent::*;

        assert_eq!(
            parse_path(r"foo\[x\][]").unwrap(),
            &[Key(r"foo[x]".into(), None), Index(None, (8, 9))]
        );
        assert_eq!(
            parse_path(r"foo\\[x]").unwrap(),
            &[Key(r"foo\".into(), None), Key("x".into(), Some((5, 7)))]
        );
        assert_eq!(
            parse_path(r"foo[ba\[ar][9]").unwrap(),
            &[
                Key("foo".into(), None),
                Key("ba[ar".into(), Some((3, 10))),
                Index(Some(9), (11, 13))
            ]
        );
        assert_eq!(
            parse_path(r"[0][foo]").unwrap(),
            &[Index(Some(0), (0, 2)), Key("foo".into(), Some((3, 7)))]
        );
        assert_eq!(
            parse_path(r"[][foo]").unwrap(),
            &[Index(None, (0, 1)), Key("foo".into(), Some((2, 6)))]
        );
        assert_eq!(
            parse_path(r"[x][0]").unwrap(),
            &[
                Key("".into(), None),
                Key("x".into(), Some((0, 2))),
                Index(Some(0), (3, 5))
            ]
        );
        assert_eq!(
            parse_path(r"[x][\0]").unwrap(),
            &[
                Key("".into(), None),
                Key("x".into(), Some((0, 2))),
                Key("0".into(), Some((3, 6)))
            ]
        );
        assert_eq!(
            parse_path(r"[x][\\0]").unwrap(),
            &[
                Key("".into(), None),
                Key("x".into(), Some((0, 2))),
                Key(r"\0".into(), Some((3, 7))),
            ]
        );
        assert_eq!(
            parse_path(r"5[x]").unwrap(),
            &[Key("5".into(), None), Key("x".into(), Some((1, 3)))]
        );

        assert!(parse_path(r"x[y]h[z]").is_err());
        assert!(parse_path(r"x[y]h").is_err());
        assert!(parse_path(r"[x][y]h").is_err());
        assert!(parse_path("foo[😀]x").is_err());
        assert!(parse_path(r"foo[bar]\[baz]").is_err());
        assert!(parse_path(r"foo\[bar][baz]").is_err());
    }
}

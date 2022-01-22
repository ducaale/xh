use std::mem;

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

#[derive(Debug, PartialEq, Eq)]
pub enum PathComponent {
    Key(String),
    Index(Option<usize>),
}

impl From<String> for PathComponent {
    fn from(value: String) -> Self {
        PathComponent::Key(value)
    }
}

impl From<usize> for PathComponent {
    fn from(value: usize) -> Self {
        PathComponent::Index(Some(value))
    }
}

/// Parse a JSON path.
///
/// A valid JSON path is `text([index])*` where `index` is either a `text`, `number` or `empty`.
///
/// Just like any `request_item`, special characters e.g `[` can be escaped with
/// a backslash character.
pub fn parse_path(raw_json_path: &str) -> Result<Vec<PathComponent>> {
    // TODO: handle escaped numbers
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
        return Ok(vec![unescape(raw_json_path, SPECIAL_CHARS).into()]);
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

    if delims[0].0 > 0 {
        // raw_json_path starts with a literal e.g `foo[x]`, `foo[5]` or `foo[]`
        json_path.push(unescape(&raw_json_path[0..delims[0].0], SPECIAL_CHARS).into());
    } else {
        let key = &raw_json_path[delims[0].0 + 1..delims[1].0];
        if !key.is_empty() && key.parse::<usize>().is_err() {
            // raw_json_path starts with `[string]`
            json_path.push("".to_string().into());
        }
    }
    for pair in delims.chunks_exact(2) {
        let path_component = &raw_json_path[pair[0].0 + 1..pair[1].0];
        if let Ok(index) = path_component.parse::<usize>() {
            json_path.push(index.into());
        } else if path_component.is_empty() {
            json_path.push(PathComponent::Index(None));
        } else if path_component.starts_with('\\') && path_component[1..].parse::<usize>().is_ok() {
            // No need to escape `path_component[1..]` as it was successfully parsed as a number before.
            json_path.push(path_component[1..].to_string().into());
        } else {
            json_path.push(unescape(path_component, SPECIAL_CHARS).into());
        }
    }

    Ok(json_path)
}

// TODO: add comment here
pub fn set_value(root: Option<Value>, path: &[PathComponent], value: Value) -> Value {
    debug_assert!(!path.is_empty(), "path should not be empty");
    match root {
        Some(Value::Object(mut obj)) => {
            let key = match &path[0] {
                PathComponent::Key(v) => v.to_string(),
                PathComponent::Index(..) => todo!("throw a nicely formatted error 1"),
            };
            if path.len() == 1 {
                obj.insert(key, value);
            } else {
                let temp = obj.remove(&key);
                let value = set_value(temp, &path[1..], value);
                obj.insert(key, value);
            };
            Value::Object(obj)
        }
        Some(Value::Array(mut arr)) => {
            let index = match path[0] {
                PathComponent::Key(..) => todo!("throw a nicely formatted error 2"),
                PathComponent::Index(Some(v)) => v,
                PathComponent::Index(None) => arr.len(),
            };
            if path.len() == 1 {
                arr_insert(&mut arr, index, value);
            } else {
                let temp = remove_from_arr(&mut arr, index);
                let value = set_value(temp, &path[1..], value);
                arr_insert(&mut arr, index, value);
            };
            Value::Array(arr)
        }
        Some(_) => {
            if path.len() == 1 {
                value
            } else {
                todo!("throw a nicely formatted error 3")
            }
        }
        None => match path[0] {
            PathComponent::Key(..) => set_value(Some(Value::Object(Map::new())), path, value),
            PathComponent::Index(..) => set_value(Some(Value::Array(vec![])), path, value),
        },
    }
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
        assert_eq!(root, json!({"foo": {"bar": {"baz": 5}}}));
    }

    #[test]
    fn deeply_nested_array() {
        let root = set_value(None, &parse_path("[0][0][1]").unwrap(), 5.into());
        assert_eq!(root, json!([[[null, 5]]]));
    }

    #[test]
    fn json_path_parser() {
        use PathComponent::*;

        assert_eq!(
            parse_path(r"foo\[x\][]").unwrap(),
            &[Key(r"foo[x]".into()), Index(None)]
        );
        assert_eq!(
            parse_path(r"foo\\[x]").unwrap(),
            &[Key(r"foo\".into()), Key("x".into())]
        );
        assert_eq!(
            parse_path(r"foo[ba\[ar][9]").unwrap(),
            &[Key("foo".into()), Key("ba[ar".into()), Index(Some(9))]
        );
        assert_eq!(
            parse_path(r"[0][foo]").unwrap(),
            &[Index(Some(0)), Key("foo".into())]
        );
        assert_eq!(
            parse_path(r"[][foo]").unwrap(),
            &[Index(None), Key("foo".into())]
        );
        assert_eq!(
            parse_path(r"[x][0]").unwrap(),
            &[Key("".into()), Key("x".into()), Index(Some(0))]
        );
        assert_eq!(
            parse_path(r"[x][\0]").unwrap(),
            &[Key("".into()), Key("x".into()), Key("0".into())]
        );
        assert_eq!(
            parse_path(r"[x][\\0]").unwrap(),
            &[Key("".into()), Key("x".into()), Key(r"\0".into())]
        );
        assert_eq!(
            parse_path(r"5[x]").unwrap(),
            &[Key("5".into()), Key("x".into())]
        );

        assert!(parse_path(r"x[y]h[z]").is_err());
        assert!(parse_path(r"x[y]h").is_err());
        assert!(parse_path(r"[x][y]h").is_err());
        assert!(parse_path("foo[😀]x").is_err());
        assert!(parse_path(r"foo[bar]\[baz]").is_err());
        assert!(parse_path(r"foo\[bar][baz]").is_err());
    }
}

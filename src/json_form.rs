use std::mem;

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

/// Parse a JSON path.
///
/// A valid JSON path is either `ident([index])+` or `([index])*`
/// where
/// - `index` -> string, number or empty
/// - `ident` -> string
///
/// Just like any `request_item`, special characters e.g `[` can be escaped with
/// a backslash character.
pub fn parse_path(raw_json_path: &str) -> Result<Vec<String>> {
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
        return Ok(vec![unescape(raw_json_path, SPECIAL_CHARS)]);
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
        // If json path doesn't start with a bracket e.g foo[x]
        json_path.push(&raw_json_path[0..delims[0].0]);
    } else {
        // Otherwise, double-check that it is either [] or [number]
        let key = dbg!(&raw_json_path[delims[0].0 + 1..delims[1].0]);
        if !key.is_empty() && key.parse::<usize>().is_err() {
            return Err(anyhow!("invalid json path {}", raw_json_path));
        }
    }
    for pair in delims.chunks_exact(2) {
        json_path.push(&raw_json_path[pair[0].0 + 1..pair[1].0]);
    }
    Ok(json_path
        .iter()
        .map(|p| unescape(p, SPECIAL_CHARS))
        .collect::<Vec<_>>())
}

// TODO: add comment here
pub fn set_value<T: AsRef<str>>(root: Value, path: &[T], value: Value) -> Value {
    debug_assert!(!path.is_empty(), "path should not be empty");
    match root {
        Value::Object(mut obj) => {
            let value = if path.len() == 1 {
                value
            } else {
                let temp = obj.remove(path[0].as_ref()).unwrap_or(Value::Null);
                set_value(temp, &path[1..], value)
            };
            obj_append(&mut obj, path[0].as_ref().to_string(), value);
            Value::Object(obj)
        }
        Value::Array(mut arr) => {
            let index = if path[0].as_ref() == "" {
                Some(arr.len())
            } else {
                path[0].as_ref().parse().ok()
            };
            if let Some(index) = index {
                let value = if path.len() == 1 {
                    value
                } else {
                    let temp1 = remove_from_arr(&mut arr, index).unwrap_or(Value::Null);
                    set_value(temp1, &path[1..], value)
                };
                arr_append(&mut arr, index, value);
                Value::Array(arr)
            } else {
                set_value(Value::Object(arr_to_obj(arr)), path, value)
            }
        }
        Value::Null => {
            if path[0].as_ref().parse::<usize>().is_ok() || path[0].as_ref() == "" {
                set_value(Value::Array(vec![]), path, value)
            } else {
                set_value(Value::Object(Map::new()), path, value)
            }
        }
        root => {
            let mut obj = Map::new();
            let value = if path.len() == 1 {
                value
            } else {
                let temp1 = obj.remove(path[0].as_ref()).unwrap_or(Value::Null);
                set_value(temp1, &path[1..], value)
            };
            obj.insert("".to_string(), root);
            obj.insert(path[0].as_ref().to_string(), value);
            Value::Object(obj)
        }
    }
}

/// Insert a value into object without overwriting existing value.
fn obj_append(obj: &mut Map<String, Value>, key: String, value: Value) {
    let old_value = obj.remove(&key).unwrap_or(Value::Null);
    match old_value {
        Value::Null => {
            obj.insert(key, value);
        }
        Value::Array(mut arr) => {
            arr.push(value);
            obj.insert(key, Value::Array(arr));
        }
        old_value => {
            obj.insert(key, Value::Array(vec![old_value, value]));
        }
    }
}

/// Insert into array at any index and without overwriting existing value.
fn arr_append(arr: &mut Vec<Value>, index: usize, value: Value) {
    while index >= arr.len() {
        arr.push(Value::Null);
    }
    let old_value = mem::replace(&mut arr[index], Value::Null);
    match old_value {
        Value::Null => {
            arr[index] = value;
        }
        Value::Array(mut temp_arr) => {
            temp_arr.push(value);
            arr[index] = Value::Array(temp_arr);
        }
        old_value => {
            arr[index] = Value::Array(vec![old_value, value]);
        }
    }
}

/// Convert array to object by using indices as keys.
fn arr_to_obj(mut arr: Vec<Value>) -> Map<String, Value> {
    let mut obj = Map::new();
    for (i, v) in arr.drain(..).enumerate() {
        obj.insert(i.to_string(), v);
    }
    obj
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

    use serde_json::{json, Value};

    #[test]
    fn deeply_nested_object() {
        let mut root = Value::Null;
        root = set_value(root, &parse_path("foo[bar][baz]").unwrap(), 5.into());
        assert_eq!(root, json!({"foo": {"bar": {"baz": 5}}}));
    }

    #[test]
    fn deeply_nested_array() {
        let mut root = Value::Null;
        root = set_value(root, &parse_path("[0][0][1]").unwrap(), 5.into());
        assert_eq!(root, json!([[[null, 5]]]));
    }

    #[test]
    fn existing_values_are_never_overwritten() {
        let mut root = Value::Null;

        root = set_value(root, &parse_path("foo").unwrap(), 5.into());
        assert_eq!(root, json!({"foo": 5}));

        root = set_value(root, &parse_path("foo").unwrap(), 7.into());
        assert_eq!(root, json!({"foo": [5, 7]}));

        root = set_value(root, &parse_path("foo").unwrap(), 7.into());
        assert_eq!(root, json!({"foo": [5, 7, 7]}));

        root = set_value(root, &parse_path("bar").unwrap(), false.into());
        assert_eq!(root, json!({"foo": [5, 7, 7], "bar": false}));

        root = set_value(root, &parse_path("bar[y]").unwrap(), 10.into());
        assert_eq!(root, json!({"foo": [5, 7, 7], "bar": {"": false, "y": 10}}));

        root = set_value(root, &parse_path("bar[y][y][y]").unwrap(), true.into());
        assert_eq!(
            root,
            json!({"foo": [5, 7, 7], "bar": {"": false, "y": {"": 10, "y": {"y": true}}}})
        );

        root = set_value(root, &parse_path("baz").unwrap(), Value::Null);
        assert_eq!(
            root,
            json!({
                "foo": [5, 7, 7],
                "bar": {"": false, "y": {"": 10, "y": {"y": true}}},
                "baz": null
            })
        );

        // ...except for null values
        root = set_value(root, &parse_path("baz[x][z]").unwrap(), 1.into());
        assert_eq!(
            root,
            json!({
                "foo": [5, 7, 7],
                "bar": {"": false, "y": {"": 10, "y": {"y": true}}},
                "baz": {"x": {"z": 1}}
            })
        );
    }

    #[test]
    fn object_array_clash() {
        let mut root = Value::Null;

        root = set_value(root, &parse_path("foo[]").unwrap(), 5.into());
        root = set_value(root, &parse_path("foo[]").unwrap(), true.into());
        assert_eq!(root, json!({"foo": [5, true]}));

        root = set_value(root, &parse_path("foo[x]").unwrap(), false.into());
        assert_eq!(root, json!({"foo": {"0": 5, "1": true, "x": false}}));

        root = set_value(root, &parse_path("foo[]").unwrap(), true.into());
        assert_eq!(
            root,
            json!({"foo": {"0": 5, "1": true, "x": false, "": true}})
        );
    }

    #[test]
    fn json_path_parser() {
        assert_eq!(parse_path(r"foo\[x\][]").unwrap(), &[r"foo[x]", ""]);
        assert_eq!(parse_path(r"foo\\[x]").unwrap(), &[r"foo\", "x"]);
        assert_eq!(
            parse_path(r"foo[ba\[ar][9]").unwrap(),
            &["foo", "ba[ar", "9"]
        );
        assert_eq!(parse_path(r"[0][foo]").unwrap(), &["0", "foo"]);
        assert_eq!(parse_path(r"[][foo]").unwrap(), &["", "foo"]);

        // Note that most of the following json_path strings are accepted by HTTPie
        // without any complaints. Hopefully, that may change in the future.
        assert!(parse_path(r"x[y]h[z]").is_err());
        assert!(parse_path(r"x[y]h").is_err());
        assert!(parse_path(r"[x][y]h").is_err());
        assert!(parse_path("foo[😀]x").is_err());
        // If a path starts with a bracket rather an identifier, it may only contain
        // a number or nothing
        assert!(parse_path(r"[x][0]").is_err());
        assert!(parse_path(r"foo[bar]\[baz]").is_err());
        assert!(parse_path(r"foo\[bar][baz]").is_err());
    }
}

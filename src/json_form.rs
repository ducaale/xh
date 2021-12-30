use std::mem;

use anyhow::{anyhow, Result};
use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

pub fn parse_path(raw_json_path: &str) -> Result<Vec<String>> {
    const SPECIAL_CHARS: &str = "[]\\";
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
        if raw_json_path.chars().count() > last_closing_bracket + 1 {
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

/// Insert a value into object without overwriting existing value
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

/// Insert into array at any index and without overwriting existing value
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

/// Convert array to object by using indices as keys
fn arr_to_obj(mut arr: Vec<Value>) -> Map<String, Value> {
    let mut obj = Map::new();
    for (i, v) in arr.drain(..).enumerate() {
        obj.insert(i.to_string(), v);
    }
    obj
}

/// Remove an element from array and replace it with `Value::Null`
fn remove_from_arr(arr: &mut Vec<Value>, index: usize) -> Option<Value> {
    if index < arr.len() {
        Some(mem::replace(&mut arr[index], Value::Null))
    } else {
        None
    }
}

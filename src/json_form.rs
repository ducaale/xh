use std::mem;

use serde_json::map::Map;
use serde_json::Value;

use crate::utils::unescape;

pub fn parse_path(raw_json_path: &str) -> Vec<String> {
    let mut delims: Vec<usize> = vec![];
    let mut backslashes = 0;

    for (i, ch) in raw_json_path.chars().enumerate() {
        if ch == '\\' {
            backslashes += 1;
        } else {
            if (ch == '[' || ch == ']') && backslashes % 2 == 0 {
                delims.push(i);
            }
            backslashes = 0;
        }
    }

    if delims.is_empty() {
        return vec![raw_json_path.to_string()];
    }

    // Missing preliminary checks
    // 1. make sure every opening bracket is followed by a closing bracket
    // 2. make sure number of delims is an even number

    let mut json_path = vec![];
    if delims[0] > 0 {
        json_path.push(&raw_json_path[0..delims[0]]);
    }
    for pair in delims.chunks_exact(2) {
        json_path.push(&raw_json_path[pair[0] + 1..pair[1]]);
    }

    json_path
        .iter()
        .map(|p| unescape(p, "[]"))
        .collect::<Vec<_>>()
}

// TODO: write comments + tests for this function
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

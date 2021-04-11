use std::collections::HashMap;
use std::convert::TryFrom;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, SET_COOKIE};
use serde::{Deserialize, Serialize};

const SESSION_CONTENT: &str = r#"
    {
        "__meta__": {
            "about": "xh session file",
            "xh": "0.9.2"
        },
        "cookies": {
            "__cfduid": "__cfduid=deafc9f3a8f236b14d9d35d487bc49fda1618150617; expires=Tue, 11-May-21 14:16:57 GMT; path=/; domain=.pie.dev; HttpOnly; SameSite=Lax"
        },
        "headers": {
            "custom": "thing",
            "customm": "thingg"
        }
    }"#;

#[derive(Debug, Serialize, Deserialize)]
struct Content {
    cookies: HashMap<String, String>,
    headers: HashMap<String, String>,
}

pub struct Session {
    host: String,
    name_or_path: String,
    pub read_only: bool,
    content: Content,
}

impl Session {
    pub fn load_session(host: String, name_or_path: String, read_only: bool) -> Self {
        let content: Content = serde_json::from_str(SESSION_CONTENT).unwrap();
        // remove expired cookies?
        Session {
            host,
            name_or_path,
            read_only,
            content,
        }
    }

    pub fn headers(&self) -> HeaderMap {
        let mut headers: HeaderMap = HeaderMap::try_from(&self.content.headers).unwrap();
        let cookies = self.content.cookies.iter()
            .map(|(_, value)| cookie::Cookie::parse(value).unwrap())
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        if !cookies.is_empty() {
            headers.entry(COOKIE).or_insert_with(|| HeaderValue::from_str(&cookies).unwrap());
        }
        headers
    }

    pub fn save_headers(&mut self, request_headers: &HeaderMap) {
        for (key, value) in request_headers.iter() {
            let key = key.as_str();
            if !key.starts_with("content-") && !key.starts_with("if-") && key != "cookie" {
                self.content
                    .headers
                    .insert(key.into(), value.to_str().unwrap().into());
            }
        }
    }

    pub fn save_auth(&mut self, request_headers: &HeaderMap) {
        if let Some(value) = request_headers.get(AUTHORIZATION) {
            self.content
                .headers
                .insert("authorization".into(), value.to_str().unwrap().into());
        }
    }

    pub fn save_cookies(&mut self, response_headers: &HeaderMap) {
        for cookie in response_headers.get_all(SET_COOKIE) {
            let raw_cookie = cookie.to_str().unwrap().to_string();
            let cookie_name = cookie::Cookie::parse(&raw_cookie).unwrap().name().to_string();
            self.content.cookies.insert(cookie_name, raw_cookie);
        }
    }

    pub fn persist(&self) {
        if self.name_or_path.contains(std::path::is_separator) {
            println!("path: {}", self.name_or_path);
        } else {
            println!("host: {} session_name: {}", self.host, self.name_or_path);
        }
        println!("{}", serde_json::to_string_pretty(&self.content).unwrap());
    }
}

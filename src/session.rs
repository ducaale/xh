use std::collections::HashMap;
use std::convert::TryFrom;

use anyhow::Result;
use cookie::Cookie;
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
    pub name_or_path: String,
    pub read_only: bool,
    content: Content,
}

impl Session {
    pub fn load_session(host: String, name_or_path: String, read_only: bool) -> Result<Self> {
        // TODO: read from file
        let content: Content = serde_json::from_str(SESSION_CONTENT)?;
        // TODO: remove expired cookies
        let s = Session {
            host,
            name_or_path,
            read_only,
            content,
        };

        Ok(s)
    }

    pub fn headers(&self) -> Result<HeaderMap> {
        let mut headers: HeaderMap = HeaderMap::try_from(&self.content.headers)?;
        let cookies = self
            .content
            .cookies
            .iter()
            .map(|(_, value)| Cookie::parse(value).unwrap())
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        if !cookies.is_empty() {
            headers
                .entry(COOKIE)
                .or_insert(HeaderValue::from_str(&cookies)?);
        }

        Ok(headers)
    }

    pub fn save_headers(&mut self, request_headers: &HeaderMap) -> Result<()> {
        for (key, value) in request_headers.iter() {
            let key = key.as_str();
            if !key.starts_with("content-") && !key.starts_with("if-") && key != "cookie" {
                self.content
                    .headers
                    .insert(key.into(), value.to_str()?.into());
            }
        }

        Ok(())
    }

    pub fn save_auth(&mut self, request_headers: &HeaderMap) -> Result<()> {
        if let Some(value) = request_headers.get(AUTHORIZATION) {
            self.content
                .headers
                .insert("authorization".into(), value.to_str()?.into());
        }

        Ok(())
    }

    pub fn save_cookies(&mut self, response_headers: &HeaderMap) -> Result<()> {
        for cookie in response_headers.get_all(SET_COOKIE) {
            let raw_cookie = cookie.to_str()?;
            let parsed_cookie = Cookie::parse(raw_cookie)?;
            self.content
                .cookies
                .insert(parsed_cookie.name().into(), raw_cookie.into());
        }

        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        if self.name_or_path.contains(std::path::is_separator) {
            println!("path: {}", self.name_or_path);
        } else {
            println!("host: {} session_name: {}", self.host, self.name_or_path);
        }

        // TODO: save to file
        println!("{}", serde_json::to_string_pretty(&self.content)?);

        Ok(())
    }
}

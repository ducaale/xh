use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, SET_COOKIE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    about: String,
    xh: String,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            about: "xh session file".into(),
            xh: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Cookie {
    name: String,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secure: Option<bool>,
}

impl Cookie {
    fn has_expired(&self) -> bool {
        match self.expires {
            Some(expires) => UNIX_EPOCH + Duration::from_millis(expires) < SystemTime::now(),
            None => false,
        }
    }
}

impl FromStr for Cookie {
    type Err = cookie::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let c = cookie::Cookie::parse(s)?;
        Ok(Cookie {
            name: c.name().into(),
            value: c.value().into(),
            expires: c
                .expires()
                .and_then(|v| v.datetime())
                .map(|v| v.unix_timestamp())
                .and_then(|v| u64::try_from(v).ok()),
            path: c.path().map(Into::into),
            secure: c.secure(),
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Content {
    #[serde(rename = "__meta__")]
    meta: Meta,
    cookies: HashMap<String, Cookie>,
    headers: HashMap<String, String>,
}

pub struct Session {
    pub path: PathBuf,
    pub read_only: bool,
    content: Content,
}

impl Session {
    pub fn load_session(host: &str, name_or_path: &str, read_only: bool) -> Result<Self> {
        let path = if name_or_path.contains(std::path::is_separator) {
            PathBuf::from(name_or_path)
        } else {
            let mut path = dirs::config_dir()
                .context("couldn't get config directory")?
                .join::<PathBuf>(["xh", "sessions", host].iter().collect());
            path.push(format!("{}.json", name_or_path));
            path
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<Content>(&content)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Content::default(),
            Err(err) => return Err(err.into()),
        };

        Ok(Session {
            path,
            read_only,
            content,
        })
    }

    pub fn headers(&self) -> Result<HeaderMap> {
        let mut headers: HeaderMap = HeaderMap::try_from(&self.content.headers)?;
        let cookies = self
            .content
            .cookies
            .values()
            .filter(|c| !c.has_expired())
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ");
        if !cookies.is_empty() {
            headers.insert(COOKIE, HeaderValue::from_str(&cookies)?);
        }
        Ok(headers)
    }

    pub fn save_headers(&mut self, request_headers: &HeaderMap) -> Result<()> {
        for (key, value) in request_headers.iter() {
            let key = key.as_str();
            if key != "cookie" && !key.starts_with("content-") && !key.starts_with("if-") {
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
            let parsed_cookie = Cookie::from_str(cookie.to_str()?)?;
            self.content
                .cookies
                .insert(parsed_cookie.name.clone(), parsed_cookie);
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        if !self.path.exists() || !self.read_only {
            if let Some(parent_path) = self.path.parent() {
                fs::create_dir_all(parent_path)?;
            }
            let mut session_file = fs::File::create(&self.path)?;
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut ser = serde_json::Serializer::with_formatter(&mut session_file, formatter);
            self.content.serialize(&mut ser)?;
            session_file.write_all(b"\n")?;
        }
        Ok(())
    }
}

use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, SET_COOKIE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    about: String,
    xh: Option<String>,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            about: "xh session file".into(),
            xh: Some(env!("CARGO_PKG_VERSION").into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Cookie {
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

    fn parse(s: &str) -> Result<(String, Self), cookie::ParseError> {
        let c = cookie::Cookie::parse(s)?;
        Ok((
            c.name().into(),
            Cookie {
                value: c.value().into(),
                expires: c
                    .expires()
                    .and_then(|v| v.datetime())
                    .map(|v| v.unix_timestamp())
                    .and_then(|v| u64::try_from(v).ok()),
                path: c.path().map(Into::into),
                secure: c.secure(),
            }
        ))
    }
}

// Note: Unlike xh, HTTPie has a dedicated section for auth info
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
    pub fn load_session(host: &str, mut name_or_path: OsString, read_only: bool) -> Result<Self> {
        let path = if is_path(&name_or_path) {
            PathBuf::from(name_or_path)
        } else {
            let mut path = dirs::config_dir()
                .context("couldn't get config directory")?
                .join::<PathBuf>(["xh", "sessions", host].iter().collect());
            name_or_path.push(".json");
            path.push(name_or_path);
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
            .iter()
            .filter(|(_, cookie)| !cookie.has_expired())
            .map(|(name, cookie)| format!("{}={}", name, cookie.value))
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
            let (name, parsed_cookie) = Cookie::parse(cookie.to_str()?)?;
            self.content
                .cookies
                .insert(name, parsed_cookie);
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

fn is_path(value: &OsString) -> bool {
    value.to_string_lossy().contains(std::path::is_separator)
}

fn insert_cookie(headers: &mut HeaderMap, cookie: HeaderValue) {
    if let Some(existing_cookie) = headers.get(COOKIE) {
        let cookie = HeaderValue::from_str(&format!(
            "{}; {}",
            existing_cookie.to_str().unwrap(),
            cookie.to_str().unwrap()
        ))
        .unwrap();
        headers.insert(COOKIE, cookie);
    } else {
        headers.insert(COOKIE, cookie);
    }
}

pub fn merge_headers(mut headers1: HeaderMap, headers2: HeaderMap) -> HeaderMap {
    let mut current_key = None;
    for (key, value) in headers2 {
        current_key = key.or(current_key);
        match current_key {
            Some(ref current_key) if current_key == COOKIE => {
                insert_cookie(&mut headers1, value);
            }
            Some(ref current_key) => {
                headers1.insert(current_key, value);
            }
            None => unreachable!(),
        }
    }
    headers1
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn can_read_httpie_session_file() -> Result<()> {
        let mut path_to_session = std::env::temp_dir();
        path_to_session.push("session1.json");
        fs::write(
            &path_to_session,
            indoc::indoc!{r#"
                {
                    "__meta__": {
                        "about": "HTTPie session file",
                        "help": "https://httpie.org/doc#sessions",
                        "httpie": "2.3.0"
                    },
                    "auth": {
                        "password": null,
                        "type": null,
                        "username": null
                    },
                    "cookies": {
                        "__cfduid": {
                            "expires": 1620239688,
                            "path": "/",
                            "secure": false,
                            "value": "d090ada9c629fc7b8bbc6dba3dde1149d1617647688"
                        }
                    },
                    "headers": {
                        "authorization": "bearer hello"
                    }
                }
            "#}
        )?;

        Session::load_session("localhost", path_to_session.into(), false)?;
        Ok(())
    }
}

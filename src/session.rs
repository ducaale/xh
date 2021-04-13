use std::collections::HashMap;
use std::convert::TryFrom;
use std::{fs, io};
use std::path::PathBuf;

use anyhow::Result;
use cookie::Cookie;
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
struct Content {
    #[serde(rename = "__meta__")]
    meta: Meta,
    // TODO: Replace String with Cookie
    // (need to use Serde's serialize_with and deserialize_with)
    cookies: HashMap<String, String>,
    headers: HashMap<String, String>,
}

impl Default for Content {
    fn default() -> Self {
        Content {
            meta: Meta::default(),
            cookies: HashMap::new(),
            headers: HashMap::new(),
        }
    }
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
                .unwrap()
                .join::<PathBuf>(["xh", "sessions", host].iter().collect());
            path.push(format!("{}.json", name_or_path));
            path
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<Content>(&content)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Content::default(),
            Err(err) => return Err(err.into())
        };

        // TODO: remove expired cookies
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
            .map(|value| Cookie::parse(value).unwrap())
            .map(|c| format!("{}={}", c.name(), c.value()))
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
            let raw_cookie = cookie.to_str()?;
            let parsed_cookie = Cookie::parse(raw_cookie)?;
            self.content
                .cookies
                .insert(parsed_cookie.name().into(), raw_cookie.into());
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        if let Some(parent_path) = self.path.parent() {
            fs::create_dir_all(parent_path)?;
        }
        let session_file = fs::File::create(&self.path)?;
        if !self.read_only {
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut ser = serde_json::Serializer::with_formatter(session_file, formatter);
            self.content.serialize(&mut ser)?;
        }
        Ok(())
    }
}

use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use cookie::Cookie;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, SET_COOKIE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Content {
    #[serde(rename = "__meta__")]
    meta: HashMap<String, String>,
    // TODO: Replace String with Cookie
    // (need to use Serde's serialize_with and deserialize_with)
    cookies: HashMap<String, String>,
    headers: HashMap<String, String>,
}

impl Default for Content {
    fn default() -> Self {
        let mut meta = HashMap::new();
        meta.insert("about".into(), "xh session file".into());
        meta.insert("xh".into(), env!("CARGO_PKG_VERSION").into());
        Content {
            meta,
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

fn ensure_file_exists(path: PathBuf) -> Result<PathBuf> {
    if path.exists() {
        Ok(path)
    } else {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::File::create(&path)?;
        Ok(path)
    }
}

impl Session {
    pub fn load_session(host: &str, name_or_path: &str, read_only: bool) -> Result<Self> {
        let path = if name_or_path.contains(std::path::is_separator) {
            ensure_file_exists(PathBuf::from(name_or_path))?
        } else {
            let mut path = dirs::config_dir()
                .unwrap()
                .join::<PathBuf>(["xh", "sessions", host, name_or_path].iter().collect());
            path.set_extension("json");
            ensure_file_exists(path)?
        };

        let content = {
            let content = fs::read_to_string(&path)?;
            if content.is_empty() {
                Content::default()
            } else {
                serde_json::from_str::<Content>(&content)?
            }
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
            .iter()
            .map(|(_, value)| Cookie::parse(value).unwrap())
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
        fs::write(&self.path, serde_json::to_string_pretty(&self.content)?)?;
        Ok(())
    }
}

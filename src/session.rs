use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::mem;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::utils::test_mode;

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    about: String,
    xh: Option<String>, // optional to be able to load HTTPie's session files
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            about: "xh session file".into(),
            xh: Some(xh_version()),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Auth {
    #[serde(rename = "type")]
    auth_type: Option<String>,
    raw_auth: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cookie {
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secure: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Content {
    #[serde(rename = "__meta__")]
    meta: Meta,
    #[serde(skip_serializing)]
    auth: Option<Auth>, // needed to maintain compatibility with HTTPie's session files
    cookies: HashMap<String, Cookie>,
    headers: HashMap<String, String>,
}

pub struct Session {
    pub path: PathBuf,
    pub read_only: bool,
    content: Content,
}

impl Session {
    pub fn load_session(url: &Url, mut name_or_path: OsString, read_only: bool) -> Result<Self> {
        let path = if is_path(&name_or_path) {
            PathBuf::from(name_or_path)
        } else {
            let mut path = config_dir()
                .context("couldn't get config directory")?
                .join::<PathBuf>(["xh", "sessions", &path_from_url(url)?].iter().collect());
            name_or_path.push(".json");
            path.push(name_or_path);
            path
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => migrate_from_old_session(serde_json::from_str::<Content>(&content)?),
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
        Ok(HeaderMap::try_from(&self.content.headers)?)
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

    pub fn cookies(&self) -> Vec<cookie_crate::Cookie> {
        let mut cookies = vec![];
        for (name, c) in self.content.cookies.iter() {
            let mut cookie_builder = cookie_crate::Cookie::build(name, &c.value);
            if let Some(expires) = c.expires {
                cookie_builder =
                    cookie_builder.expires(time::OffsetDateTime::from_unix_timestamp(expires));
            }
            if let Some(ref path) = c.path {
                cookie_builder = cookie_builder.path(path);
            }
            if let Some(secure) = c.secure {
                cookie_builder = cookie_builder.secure(secure);
            }
            cookies.push(cookie_builder.finish());
        }
        cookies
    }

    pub fn save_cookies(&mut self, cookies: Vec<cookie_crate::Cookie>) {
        self.content.cookies.clear();
        for cookie in cookies {
            self.content.cookies.insert(
                cookie.name().into(),
                Cookie {
                    value: cookie.value().into(),
                    expires: cookie
                        .expires()
                        .and_then(|v| v.datetime())
                        .map(|v| v.unix_timestamp()),
                    path: cookie.path().map(Into::into),
                    secure: cookie.secure(),
                },
            );
        }
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

fn config_dir() -> Option<PathBuf> {
    if test_mode() {
        Some(std::env::temp_dir())
    } else {
        dirs::config_dir()
    }
}

fn xh_version() -> String {
    if test_mode() {
        "0.0.0".into()
    } else {
        env!("CARGO_PKG_VERSION").into()
    }
}

fn path_from_url(url: &Url) -> Result<String> {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => Ok(format!("{}_{}", host, port)),
        (Some(host), None) => Ok(host.into()),
        (None, _) => Err(anyhow!("couldn't extract host from url")),
    }
}

fn migrate_from_old_session(mut content: Content) -> Content {
    let auth = mem::take(&mut content.auth);
    if let Some(Auth {
        auth_type: Some(ref auth_type),
        raw_auth: Some(ref raw_auth),
    }) = auth
    {
        if auth_type.as_str() == "basic" {
            content
                .headers
                .entry("authorization".into())
                .or_insert_with(|| format!("Basic {}", base64::encode(raw_auth)));
        }
    }
    // meta will be useful in the future for migrating old session files
    // but for time being, we're going to just override it
    content.meta = Meta::default();
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::random_string;
    use anyhow::Result;

    #[test]
    fn can_read_httpie_session_file() -> Result<()> {
        let mut path_to_session = std::env::temp_dir();
        let file_name = random_string();
        path_to_session.push(file_name);
        fs::write(
            &path_to_session,
            indoc::indoc! {r#"
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
            "#},
        )?;

        Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;
        Ok(())
    }

    #[test]
    fn can_deserialize_auth_section() -> Result<()> {
        let mut path_to_session = std::env::temp_dir();
        let file_name = random_string();
        path_to_session.push(file_name);
        fs::write(
            &path_to_session,
            indoc::indoc! {r#"
                {
                    "__meta__": {
                        "about": "HTTPie session file",
                        "help": "https://httpie.org/doc#sessions",
                        "httpie": "2.3.0"
                    },
                    "auth": {
                        "type": "basic",
                        "raw_auth": "user:pass"
                    },
                    "cookies": {},
                    "headers": {}
                }
            "#},
        )?;

        let session = Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;

        assert_eq!(
            session.content.headers.get("authorization"),
            Some(&"Basic dXNlcjpwYXNz".to_string()),
        );

        Ok(())
    }

    #[test]
    fn can_read_xh_session_file() -> Result<()> {
        let mut path_to_session = std::env::temp_dir();
        let file_name = random_string();
        path_to_session.push(file_name);
        fs::write(
            &path_to_session,
            indoc::indoc! {r#"
                {
                    "__meta__": {
                        "about": "HTTPie session file",
                        "xh": "0.9.2"
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
            "#},
        )?;

        Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;
        Ok(())
    }
}

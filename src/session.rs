use std::collections::HashMap;
use std::convert::TryInto;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use reqwest::header::HeaderMap;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::utils::{config_dir, test_mode};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Meta {
    Xh { about: String, xh: String },
    Httpie { httpie: String },
    Other,
}

impl Default for Meta {
    fn default() -> Self {
        Meta::Xh {
            about: "xh session file".into(),
            xh: xh_version(),
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Auth {
    #[serde(rename = "type")]
    auth_type: Option<String>,
    raw_auth: Option<String>,
}

// Unlike xh, HTTPie serializes path, secure and expires with defaults of "/", false, and null respectively.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Cookie {
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secure: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Headers {
    // old headers format kept for backward compatibility
    Map(HashMap<String, String>),
    // new header format that supports duplicate keys
    List(Vec<Header>),
}

impl Default for Headers {
    fn default() -> Self {
        Headers::List(Vec::new())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Content {
    #[serde(rename = "__meta__")]
    meta: Meta,
    auth: Auth,
    cookies: HashMap<String, Cookie>,
    headers: Headers,
}

impl Content {
    fn migrate(mut self) -> Self {
        self.meta = Meta::default();
        if let Headers::Map(headers) = self.headers {
            self.headers = Headers::List(
                headers
                    .into_iter()
                    .map(|(key, value)| Header { name: key, value })
                    .collect(),
            );
        }

        self
    }
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
                .join::<PathBuf>(["sessions", &path_from_url(url)?].iter().collect());
            name_or_path.push(".json");
            path.push(name_or_path);
            path
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<Content>(&content)?.migrate(),
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
        match &self.content.headers {
            Headers::Map(headers) => Ok(headers.try_into()?),
            Headers::List(headers) => headers
                .iter()
                .map(|Header { name, value }| Ok((name.try_into()?, value.try_into()?)))
                .collect(),
        }
    }

    pub fn save_headers(&mut self, request_headers: &HeaderMap) -> Result<()> {
        if let Headers::List(ref mut headers) = self.content.headers {
            headers.clear();
        }

        for (key, value) in request_headers.iter() {
            let key = key.as_str();
            // HTTPie ignores headers that are specific to a particular request e.g content-length
            // see https://github.com/httpie/httpie/commit/e09b74021c9c955fd7c3bab11f22801aaf9dc1b8
            // we will also ignore cookies as they are taken care of by save_cookies()
            if key != "cookie" && !key.starts_with("content-") && !key.starts_with("if-") {
                match self.content.headers {
                    Headers::Map(ref mut headers) => {
                        headers.insert(key.into(), value.to_str()?.into());
                    }
                    Headers::List(ref mut headers) => {
                        headers.push(Header {
                            name: key.into(),
                            value: value.to_str()?.into(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    pub fn auth(&self) -> Result<Option<auth::Auth>> {
        if let Auth {
            auth_type: Some(auth_type),
            raw_auth: Some(raw_auth),
        } = &self.content.auth
        {
            match auth_type.as_str() {
                "basic" => {
                    let (username, password) = auth::parse_auth(raw_auth, "")?;
                    Ok(Some(auth::Auth::Basic(username, password)))
                }
                "digest" => {
                    let (username, password) = auth::parse_auth(raw_auth, "")?;
                    Ok(Some(auth::Auth::Digest(
                        username,
                        password.unwrap_or_else(|| "".into()),
                    )))
                }
                "bearer" => Ok(Some(auth::Auth::Bearer(raw_auth.into()))),
                _ => Err(anyhow!("Unknown auth type {}", raw_auth)),
            }
        } else {
            Ok(None)
        }
    }

    pub fn save_auth(&mut self, auth: &auth::Auth) {
        match auth {
            auth::Auth::Basic(username, password) => {
                let password = password.as_deref().unwrap_or("");
                self.content.auth = Auth {
                    auth_type: Some("basic".into()),
                    raw_auth: Some(format!("{}:{}", username, password)),
                }
            }
            auth::Auth::Digest(username, password) => {
                self.content.auth = Auth {
                    auth_type: Some("digest".into()),
                    raw_auth: Some(format!("{}:{}", username, password)),
                }
            }
            auth::Auth::Bearer(token) => {
                self.content.auth = Auth {
                    auth_type: Some("bearer".into()),
                    raw_auth: Some(token.into()),
                }
            }
        }
    }

    pub fn cookies(&self) -> Vec<cookie_crate::Cookie> {
        let mut cookies = vec![];
        for (name, c) in &self.content.cookies {
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

fn xh_version() -> String {
    if test_mode() {
        "0.0.0".into()
    } else {
        env!("CARGO_PKG_VERSION").into()
    }
}

fn is_path(value: &OsString) -> bool {
    value.to_string_lossy().contains(std::path::is_separator)
}

fn path_from_url(url: &Url) -> Result<String> {
    match (url.host_str(), url.port()) {
        (Some("."), _) | (Some(".."), _) | (None, _) => {
            Err(anyhow!("couldn't extract host from url"))
        }
        (Some(host), Some(port)) => Ok(format!("{}_{}", host, port)),
        (Some(host), None) => Ok(host.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use reqwest::header::HeaderValue;

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
                        "hello": "world"
                    }
                }
            "#},
        )?;

        let session = Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;

        assert_eq!(
            session.headers()?.get("hello"),
            Some(&HeaderValue::from_static("world")),
        );

        assert_eq!(
            session.content.auth,
            Auth {
                auth_type: None,
                raw_auth: None
            },
        );

        let expected_cookie = serde_json::from_str::<Cookie>(
            r#"
                {
                    "expires": 1620239688,
                    "path": "/",
                    "secure": false,
                    "value": "d090ada9c629fc7b8bbc6dba3dde1149d1617647688"
                }
            "#,
        )?;
        assert_eq!(
            session.content.cookies.get("__cfduid"),
            Some(&expected_cookie)
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
                        "about": "xh session file",
                        "xh": "0.0.0"
                    },
                    "auth": {
                        "raw_auth": "secret-token",
                        "type": "bearer"
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
                        "hello": "world"
                    }
                }
            "#},
        )?;

        let session = Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;

        assert_eq!(
            session.headers()?.get("hello"),
            Some(&HeaderValue::from_static("world")),
        );

        assert_eq!(
            session.content.auth,
            Auth {
                auth_type: Some("bearer".into()),
                raw_auth: Some("secret-token".into())
            },
        );

        let expected_cookie = serde_json::from_str::<Cookie>(
            r#"
                {
                    "expires": 1620239688,
                    "path": "/",
                    "secure": false,
                    "value": "d090ada9c629fc7b8bbc6dba3dde1149d1617647688"
                }
            "#,
        )?;
        assert_eq!(
            session.content.cookies.get("__cfduid"),
            Some(&expected_cookie)
        );
        Ok(())
    }

    #[test]
    fn can_read_session_with_duplicate_keys() -> Result<()> {
        let mut path_to_session = std::env::temp_dir();
        let file_name = random_string();
        path_to_session.push(file_name);
        fs::write(
            &path_to_session,
            indoc::indoc! {r#"
                {
                    "__meta__": {
                        "about": "xh session file",
                        "xh": "0.0.0"
                    },
                    "auth": {},
                    "cookies": {},
                    "headers": [
                        { "name": "hello", "value": "world" },
                        { "name": "hello", "value": "people" }
                    ]
                }
            "#},
        )?;

        let session = Session::load_session(
            &Url::parse("http://localhost")?,
            path_to_session.into(),
            false,
        )?;

        assert_eq!(
            session
                .headers()?
                .get_all("hello")
                .into_iter()
                .collect::<Vec<_>>(),
            [
                &HeaderValue::from_static("world"),
                &HeaderValue::from_static("people")
            ],
        );

        Ok(())
    }
}

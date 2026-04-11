use std::env::current_dir;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use anyhow::{Context as _, Result, anyhow};
use regex_lite::Regex;
use reqwest::StatusCode;
use reqwest::blocking::{Request, Response};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, WWW_AUTHENTICATE};
use serde::{Deserialize, Serialize};

use crate::cli::AuthType;
use crate::middleware::{Context, Middleware};
use crate::netrc;
use crate::utils::clone_request;

#[derive(Debug, PartialEq, Eq)]
pub enum Auth {
    Bearer(String),
    Basic(String, Option<String>),
    Digest(String, String),
    Plugin(AuthPlugin),
}

impl Auth {
    pub fn from_str(auth: &str, auth_type: AuthType, host: &str) -> Result<Auth> {
        match auth_type {
            AuthType::Basic => {
                let (username, password) = parse_auth(auth, host)?;
                Ok(Auth::Basic(username, password))
            }
            AuthType::Digest => {
                let (username, password) = parse_auth(auth, host)?;
                Ok(Auth::Digest(
                    username,
                    password.unwrap_or_else(|| "".into()),
                ))
            }
            AuthType::Bearer => Ok(Auth::Bearer(auth.into())),
            AuthType::Plugin(name) => Ok(Auth::Plugin(AuthPlugin::new(name, auth.into()))),
        }
    }

    pub fn from_netrc(auth_type: AuthType, entry: netrc::Entry) -> Option<Auth> {
        match auth_type {
            AuthType::Basic => Some(Auth::Basic(entry.login?, Some(entry.password))),
            AuthType::Bearer => Some(Auth::Bearer(entry.password)),
            AuthType::Digest => Some(Auth::Digest(entry.login?, entry.password)),
            AuthType::Plugin(..) => None,
        }
    }
}

pub fn parse_auth(auth: &str, host: &str) -> io::Result<(String, Option<String>)> {
    if let Some(cap) = Regex::new(r"^([^:]*):$").unwrap().captures(auth) {
        Ok((cap[1].to_string(), None))
    } else if let Some(cap) = Regex::new(r"^(.+?):(.+)$").unwrap().captures(auth) {
        let username = cap[1].to_string();
        let password = cap[2].to_string();
        Ok((username, Some(password)))
    } else {
        let username = auth.to_string();
        let prompt = format!("http: password for {username}@{host}: ");
        let password = rpassword::prompt_password(prompt)?;
        Ok((username, Some(password)))
    }
}

pub struct DigestAuthMiddleware<'a> {
    username: &'a str,
    password: &'a str,
}

impl<'a> DigestAuthMiddleware<'a> {
    pub fn new(username: &'a str, password: &'a str) -> Self {
        DigestAuthMiddleware { username, password }
    }
}

impl Middleware for DigestAuthMiddleware<'_> {
    fn handle(&mut self, mut ctx: Context, mut request: Request) -> Result<Response> {
        let mut response = self.next(&mut ctx, clone_request(&mut request)?)?;
        match response.headers().get(WWW_AUTHENTICATE) {
            Some(wwwauth) if response.status() == StatusCode::UNAUTHORIZED => {
                let mut context = digest_auth::AuthContext::new(
                    self.username,
                    self.password,
                    request.url().path(),
                );
                if let Some(cnonc) = std::env::var_os("XH_TEST_DIGEST_AUTH_CNONCE") {
                    context.set_custom_cnonce(cnonc.to_string_lossy().to_string());
                }
                let mut prompt = digest_auth::parse(wwwauth.to_str()?)?;
                let answer = prompt.respond(&context)?.to_header_string();
                request
                    .headers_mut()
                    .insert(AUTHORIZATION, HeaderValue::from_str(&answer)?);
                self.print(&mut ctx, &mut response, &mut request)?;
                Ok(self.next(&mut ctx, request)?)
            }
            _ => Ok(response),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthPlugin {
    name: String,
    auth: String,
}

impl AuthPlugin {
    pub fn new(name: String, auth: String) -> Self {
        AuthPlugin { name: name, auth }
    }
}

#[derive(Debug, Serialize)]
struct Meta {
    xh: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginOutput {
    add_headers: Vec<Header>,
}

#[derive(Debug, Serialize)]
struct XhOutput {
    #[serde(rename = "__meta__")]
    meta: Meta,
    url: String,
    auth: Vec<String>,
    headers: Vec<Header>,
    current_dir: PathBuf,
}

impl AuthPlugin {
    pub fn authenticate(&self, request: &mut Request) -> Result<()> {
        // TODO: add tests. See https://stackoverflow.com/questions/77120851/rust-mocking-stdprocesschild-for-test
        let mut child = process::Command::new(format!("xh-plugin-{}", self.name))
            .env("XH_AUTH_PLUGIN", "1")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()?;

        let xh_output = XhOutput {
            meta: Meta {
                xh: env!("CARGO_PKG_VERSION"),
            },
            url: request.url().to_string(),
            // TODO: support passing multiple --auth
            auth: vec![self.auth.to_string()],
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| {
                    Ok(Header {
                        name: name.to_string(),
                        value: value.to_str()?.into(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            current_dir: current_dir()?,
        };

        let child_stdin = child.stdin.as_mut().unwrap();
        child_stdin.write_all(&serde_json::to_vec(&xh_output)?)?;

        let output = child
            .wait_with_output()
            .context("Failed to wait for plugin output")?;

        if !output.status.success() {
            if let Some(code) = output.status.code() {
                return Err(anyhow!("plugin exited with exit code {}", code));
            } else {
                return Err(anyhow!("plugin exited no exit code"));
            }
        }

        // TODO: depending on exit code or pluginOutput, call plugin again with request body

        let plugin_output = serde_json::from_slice::<PluginOutput>(&output.stdout)?;
        request.headers_mut().extend(
            plugin_output
                .add_headers
                .iter()
                .map(|Header { name, value }| Ok((name.try_into()?, value.try_into()?)))
                .collect::<Result<HeaderMap>>()?,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        let expected = vec![
            ("user:", ("user", None)),
            ("user:password", ("user", Some("password"))),
            ("user:pass:with:colons", ("user", Some("pass:with:colons"))),
            (":", ("", None)),
        ];
        for (input, output) in expected {
            let (user, pass) = parse_auth(input, "").unwrap();
            assert_eq!(output, (user.as_str(), pass.as_deref()));
        }
    }
}

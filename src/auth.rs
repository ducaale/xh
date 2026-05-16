use std::env::current_dir;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use anyhow::{Context as _, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as base64_standard};
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
            AuthType::Plugin(..) => unreachable!(),
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
    auth: Vec<String>,
}

impl AuthPlugin {
    pub fn new(name: String, auth: Vec<String>) -> Self {
        AuthPlugin { name: name, auth }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PluginOutput {
    ModifyRequest { add_headers: Vec<Header> },
    GetRequestWithBody,
}

#[derive(Debug, Serialize)]
struct Meta {
    xh: &'static str,
}

#[derive(Debug, Serialize)]
struct XhOutput<'a> {
    #[serde(rename = "__meta__")]
    meta: Meta,
    method: String,
    url: String,
    headers: Vec<Header>,
    base64_body: Option<String>,
    auth: &'a [String],
    current_dir: PathBuf,
}

impl<'a> XhOutput<'a> {
    fn new(request: &mut Request, include_body: bool, auth: &'a [String]) -> Result<Self> {
        let mut base64_body = None;
        if include_body {
            if let Some(body) = request.body_mut() {
                let body = body.buffer()?;
                base64_body = Some(base64_standard.encode(body));
            }
        }

        Ok(XhOutput {
            meta: Meta {
                xh: env!("CARGO_PKG_VERSION"),
            },
            method: request.method().to_string(),
            url: request.url().to_string(),
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
            base64_body,
            auth,
            current_dir: current_dir()?,
        })
    }
}

impl AuthPlugin {
    pub fn authenticate(&self, request: &mut Request) -> Result<()> {
        let mut include_body = false;
        for _ in 0..2 {
            log::debug!("Spawning plugin xh-plugin-{}", self.name);
            let mut child = process::Command::new(format!("xh-plugin-{}", self.name))
                .env("XH_AUTH_PLUGIN", "1")
                .env("XH_AUTH_PLUGIN_BODY", if include_body { "1" } else { "0" })
                .stdin(process::Stdio::piped())
                .stdout(process::Stdio::piped())
                .spawn()
                .map_err(|e| anyhow!("Unable to spawn plugin 'xh-plugin-{}': {}", self.name, e))?;

            let xh_output = XhOutput::new(request, include_body, &self.auth)?;

            let child_stdin = child.stdin.as_mut().unwrap();
            if include_body {
                log::debug!("Sending request including body to plugin's stdin");
            } else {
                log::debug!("Sending request without body to plugin's stdin");
            }
            child_stdin.write_all(&serde_json::to_vec(&xh_output)?)?;

            let output = child
                .wait_with_output()
                .context("Failed to wait for plugin output")?;

            if !output.status.success() {
                if let Some(code) = output.status.code() {
                    return Err(anyhow!("Plugin exited with exit code {}", code));
                } else {
                    return Err(anyhow!("Plugin exited no exit code"));
                }
            }

            let plugin_output = serde_json::from_slice::<PluginOutput>(&output.stdout)?;
            match plugin_output {
                PluginOutput::ModifyRequest { add_headers } => {
                    log::debug!("Received 'add_headers' from plugin");
                    request.headers_mut().extend(
                        add_headers
                            .iter()
                            .map(|Header { name, value }| Ok((name.try_into()?, value.try_into()?)))
                            .collect::<Result<HeaderMap>>()?,
                    );
                    break;
                }
                PluginOutput::GetRequestWithBody => {
                    log::debug!("Received 'get_request_with_body' from plugin");
                    include_body = true;
                }
            }
        }

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

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
    state: serde_json::Value,
}

impl AuthPlugin {
    pub fn new(name: String, auth: Vec<String>) -> Self {
        AuthPlugin {
            name: name,
            auth,
            state: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PluginResponse {
    remove_headers: Option<Vec<String>>,
    add_headers: Option<Vec<Header>>,
    set_state: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct NextRequest {
    method: String,
    url: String,
    headers: Vec<Header>,
    body_base64: Option<String>,
}

#[derive(Debug, Serialize)]
struct PluginInput<'a, 'b> {
    next_request: NextRequest,
    auth: &'a [String],
    state: &'b serde_json::Value,
    current_dir: PathBuf,
}

impl<'a, 'b> PluginInput<'a, 'b> {
    fn new(
        next_request: &mut Request,
        auth: &'a [String],
        state: &'b serde_json::Value,
    ) -> Result<Self> {
        let mut body_base64 = None;
        if let Some(body) = next_request.body_mut() {
            let body = body.buffer()?;
            body_base64 = Some(base64_standard.encode(body));
        }

        let plugin_input = PluginInput {
            next_request: NextRequest {
                method: next_request.method().to_string(),
                url: next_request.url().to_string(),
                headers: next_request
                    .headers()
                    .iter()
                    .map(|(name, value)| {
                        Ok(Header {
                            name: name.to_string(),
                            value: value.to_str()?.into(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                body_base64,
            },
            auth,
            state,
            current_dir: current_dir()?,
        };

        Ok(plugin_input)
    }
}

impl AuthPlugin {
    pub fn authenticate(&mut self, next_request: &mut Request) -> Result<()> {
        log::debug!("Spawning plugin xh-plugin-{}", self.name);
        let mut child = process::Command::new(format!("xh-plugin-{}", self.name))
            .env("XH_AUTH_PLUGIN", "1")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Unable to spawn plugin 'xh-plugin-{}': {}", self.name, e))?;

        let plugin_input = PluginInput::new(next_request, &self.auth, &self.state)?;

        let child_stdin = child.stdin.as_mut().unwrap();
        log::debug!("Writing to plugin's stdin");
        child_stdin.write_all(&serde_json::to_vec(&plugin_input)?)?;

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

        let plugin_output = serde_json::from_slice::<PluginResponse>(&output.stdout)?;
        if let Some(headers_to_remove) = plugin_output.remove_headers {
            for header in headers_to_remove {
                next_request.headers_mut().remove(header);
            }
        }
        if let Some(headers_to_add) = plugin_output.add_headers {
            next_request.headers_mut().extend(
                headers_to_add
                    .iter()
                    .map(|Header { name, value }| Ok((name.try_into()?, value.try_into()?)))
                    .collect::<Result<HeaderMap>>()?,
            );
        }
        if let Some(state) = plugin_output.set_state {
            self.state = state
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

use std::ffi::OsString;
use std::io::{self, Write};
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
use crate::utils::{clone_request, is_path};

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
    name_or_path: OsString,
    auth: Vec<String>,
}

impl AuthPlugin {
    pub fn new(name_or_path: OsString, auth: Vec<String>) -> Self {
        AuthPlugin { name_or_path, auth }
    }
}

#[derive(Debug, Deserialize)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PluginResponse {
    add_headers: Vec<Header>,
}

#[derive(Debug, Serialize)]
struct PluginInput<'a> {
    url: &'a str,
    auth: &'a [String],
}

impl<'a> PluginInput<'a> {
    fn new(url: &'a str, auth: &'a [String]) -> Self {
        PluginInput { url, auth }
    }
}

impl AuthPlugin {
    pub fn headers(&self, url: &str) -> Result<HeaderMap> {
        let plugin_input = PluginInput::new(url, &self.auth);

        let plugin_output = serde_json::from_slice::<PluginResponse>(
            &self.exec(&serde_json::to_vec(&plugin_input)?)?,
        )?;

        plugin_output
            .add_headers
            .iter()
            .map(|Header { name, value }| Ok((name.try_into()?, value.try_into()?)))
            .collect::<Result<HeaderMap>>()
    }

    fn exec(&self, plugin_input: &[u8]) -> Result<Vec<u8>> {
        let plugin_path = if is_path(&self.name_or_path) {
            std::path::PathBuf::from(&self.name_or_path)
        } else {
            std::path::PathBuf::from(format!("xh-{}", self.name_or_path.to_string_lossy()))
        };

        log::debug!("Spawning plugin {:?}", plugin_path);
        let mut child = process::Command::new(&plugin_path)
            .env("XH_PLUGIN", "simple_auth")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Unable to spawn plugin {:?}: {}", plugin_path, e))?;

        let child_stdin = child.stdin.as_mut().unwrap();
        log::debug!("Writing to plugin's stdin");
        child_stdin.write_all(plugin_input)?;

        let output = child
            .wait_with_output()
            .context("Failed to wait for plugin output")?;

        if !output.status.success() {
            // TODO: support standardised way of reporting errors from plugin
            if let Some(code) = output.status.code() {
                return Err(anyhow!("Plugin exited with exit code {}", code));
            } else {
                return Err(anyhow!("Plugin exited no exit code"));
            }
        }

        Ok(output.stdout)
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

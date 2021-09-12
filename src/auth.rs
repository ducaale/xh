use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::Result;
use dirs::home_dir;
use netrc_rs::Netrc;
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE};
use reqwest::StatusCode;

use crate::middleware::{Middleware, Next};
use crate::regex;

// TODO: move this to utils.rs
fn clone_request(request: &mut Request) -> Result<Request> {
    if let Some(b) = request.body_mut().as_mut() {
        b.buffer()?;
    }
    // This doesn't copy the contents of the buffer, cloning requests is cheap
    // https://docs.rs/bytes/1.0.1/bytes/struct.Bytes.html
    Ok(request.try_clone().unwrap()) // guaranteed to not fail if body is already buffered
}

pub fn parse_auth(auth: String, host: &str) -> io::Result<(String, Option<String>)> {
    if let Some(cap) = regex!(r"^([^:]*):$").captures(&auth) {
        Ok((cap[1].to_string(), None))
    } else if let Some(cap) = regex!(r"^(.+?):(.+)$").captures(&auth) {
        let username = cap[1].to_string();
        let password = cap[2].to_string();
        Ok((username, Some(password)))
    } else {
        let username = auth;
        let prompt = format!("http: password for {}@{}: ", username, host);
        let password = rpassword::read_password_from_tty(Some(&prompt))?;
        Ok((username, Some(password)))
    }
}

fn get_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(path) = env::var_os("XH_TEST_MODE_WIN_HOME_DIR") {
        return Some(PathBuf::from(path));
    }

    home_dir()
}

fn netrc_path() -> Option<PathBuf> {
    match env::var_os("NETRC") {
        Some(path) => {
            let pth = PathBuf::from(path);
            if pth.exists() {
                Some(pth)
            } else {
                None
            }
        }
        None => {
            if let Some(hd_path) = get_home_dir() {
                [".netrc", "_netrc"]
                    .iter()
                    .map(|f| hd_path.join(f))
                    .find(|p| p.exists())
            } else {
                None
            }
        }
    }
}

pub fn read_netrc() -> Option<String> {
    if let Some(netrc_path) = netrc_path() {
        if let Ok(result) = fs::read_to_string(netrc_path) {
            return Some(result);
        }
    };

    None
}

pub fn auth_from_netrc(machine: &str, netrc: &str) -> Option<(String, Option<String>)> {
    if let Ok(netrc) = Netrc::parse_borrow(&netrc, false) {
        return netrc
            .machines
            .into_iter()
            .filter_map(|mach| match mach.name {
                Some(name) if name == machine => {
                    let user = mach.login.unwrap_or_else(|| "".to_string());
                    Some((user, mach.password))
                }
                _ => None,
            })
            .last();
    }

    None
}

pub struct DigestAuth<'a> {
    username: &'a str,
    password: &'a str,
}

impl<'a> DigestAuth<'a> {
    pub fn new(username: &'a str, password: &'a str) -> Self {
        DigestAuth { username, password }
    }
}

impl<'a> Middleware for DigestAuth<'a> {
    fn handle(&mut self, mut request: Request, mut next: Next) -> Result<Response> {
        let response = next.run(clone_request(&mut request)?)?;
        match response.headers().get(WWW_AUTHENTICATE) {
            Some(wwwauth) if response.status() == StatusCode::UNAUTHORIZED => {
                let context = digest_auth::AuthContext::new(
                    self.username,
                    self.password,
                    request.url().path(),
                );
                let mut prompt = digest_auth::parse(wwwauth.to_str()?)?;
                let answer = prompt.respond(&context)?.to_header_string();
                request
                    .headers_mut()
                    .insert(AUTHORIZATION, HeaderValue::from_str(&answer)?);
                Ok(next.run(request)?)
            }
            _ => Ok(response),
        }
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
            let (user, pass) = parse_auth(input.to_string(), "").unwrap();
            assert_eq!(output, (user.as_str(), pass.as_deref()));
        }
    }

    #[test]
    fn netrc() {
        let good_netrc = "machine example.com\nlogin user\npassword pass";
        let malformed_netrc = "I'm a malformed netrc!";
        let missing_login = "machine example.com\npassword pass";
        let missing_pass = "machine example.com\nlogin user\n";

        let expected = vec![
            (
                "example.com",
                good_netrc,
                Some(("user".to_string(), Some("pass".to_string()))),
            ),
            ("example.org", good_netrc, None),
            ("example.com", malformed_netrc, None),
            (
                "example.com",
                missing_login,
                Some(("".to_string(), Some("pass".to_string()))),
            ),
            (
                "example.com",
                missing_pass,
                Some(("user".to_string(), None)),
            ),
        ];

        for (machine, netrc, output) in expected {
            assert_eq!(output, auth_from_netrc(machine, netrc));
        }
    }
}

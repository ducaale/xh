use regex::Regex;

use crate::{AuthType, Url};

#[derive(Debug, Clone)]
pub enum Auth {
    Bearer(String),
    Basic(String, Option<String>),
}

impl Auth {
    pub fn new(auth: Option<String>, auth_type: Option<AuthType>, url: &Url) -> Option<Auth> {
        let auth_type = auth_type.unwrap_or(AuthType::Basic);
        let auth = match auth {
            Some(auth) if !auth.is_empty() => auth,
            _ => {
                return None;
            }
        };

        match auth_type {
            AuthType::Basic => {
                let re = Regex::new(r"^(.+?):(.*)$").unwrap();
                if let Some(cap) = re.captures(&auth) {
                    let username = cap[1].to_string();
                    let password = if !cap[2].is_empty() {
                        Some(cap[2].to_string())
                    } else {
                        None
                    };
                    Some(Auth::Basic(username, password))
                } else {
                    let username = auth;
                    let prompt =
                        format!("http: password for {}@{}: ", username, url.host().unwrap());
                    let password = rpassword::read_password_from_tty(Some(&prompt)).unwrap();
                    Some(Auth::Basic(username, Some(password)))
                }
            }
            AuthType::Bearer => Some(Auth::Bearer(auth)),
        }
    }
}

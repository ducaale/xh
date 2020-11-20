use regex::Regex;
use crate::AuthType;

#[derive(Debug, Clone)]
pub enum Auth {
    Bearer(String),
    Basic(String, Option<String>)
}

impl Auth {
    pub fn new(auth: Option<String>, auth_type: Option<AuthType>) -> Option<Auth> {
        let auth_type = auth_type.unwrap_or(AuthType::Basic);
        let auth = match auth {
            Some(auth) if !auth.is_empty() => auth,
            _ => { return None; }
        };

        match auth_type {
            AuthType::Basic => {
                let re = Regex::new(r"^(.+?):(.*)$").unwrap();
                if let Some(cap) = re.captures(&auth) {
                    // TODO: prompt for password when cap[2] is empty
                    Some(Auth::Basic(cap[1].to_string(), Some(cap[2].to_string())))
                } else {
                    Some(Auth::Basic(auth, None))
                }
            }
            AuthType::Bearer => Some(Auth::Bearer(auth))
        }
    }
}

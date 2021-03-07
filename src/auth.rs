use std::env;
use std::io;
use std::path::PathBuf;

use crate::regex;
use dirs::home_dir;
use netrc_rs::Netrc;
use std::fs;

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

fn netrc_path() -> Option<PathBuf> {
    match env::var("NETRC") {
        Ok(path) => {
            let pth = PathBuf::from(path);
            if pth.exists() {
                Some(pth)
            } else {
                None
            }
        }
        Err(_) => {
            if let Some(hd_path) = home_dir() {
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
        let netrc = "machine example.com\nlogin user\npassword pass";

        let expected = vec![
            (
                "example.com",
                Some(("user".to_string(), Some("pass".to_string()))),
            ),
            ("example.org", None),
        ];

        for (machine, output) in expected {
            assert_eq!(output, auth_from_netrc(machine, netrc));
        }
    }
}

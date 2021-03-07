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

fn get_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "win")]
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

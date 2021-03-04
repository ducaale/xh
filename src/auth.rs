use std::io;

use crate::regex;
use dirs::home_dir;
use netrc_rs::Netrc;
use std::fs::File;
use std::io::Read;

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

pub fn read_netrc() -> Option<String> {
    let mut netrc_buf = String::new();
    if let Some(mut hd_path) = home_dir() {
        hd_path.push(".netrc");
        if let Ok(mut netrc_file) = File::open(hd_path) {
            if netrc_file.read_to_string(&mut netrc_buf).is_ok() {
                return Some(netrc_buf);
            };
        };
    };

    None
}

pub fn auth_from_netrc(machine: &str, netrc: String) -> Option<(String, Option<String>)> {
    if let Ok(netrc) = Netrc::parse_borrow(&netrc, false) {
        let mut auths: Vec<(String, Option<String>)> = netrc
            .machines
            .iter()
            .filter_map(|mach| {
                if let Some(name) = &mach.name {
                    if name.ends_with(machine) {
                        let user = match mach.login {
                            Some(ref user) => user.clone(),
                            None => "".to_string(),
                        };
                        let password = match mach.password {
                            Some(ref pwd) => Some(pwd.clone()),
                            None => None,
                        };
                        Some((user, password))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        if !auths.is_empty() {
            return auths.pop();
        }
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
}

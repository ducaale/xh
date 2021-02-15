use std::io;

use crate::regex;

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

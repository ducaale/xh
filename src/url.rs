use anyhow::Result;
use regex::Regex;

pub struct Url(pub reqwest::Url);

impl Url {
    pub fn new(url: String, default_scheme: Option<String>) -> Result<Url> {
        lazy_static::lazy_static! {
            static ref RE: Regex = Regex::new("[a-zA-Z]://.+").unwrap();
        }

        let default_scheme = default_scheme.as_deref().unwrap_or("http://");
        Ok(if url.starts_with(':') {
            let url = format!("{}{}{}", default_scheme, "localhost", url);
            Url(reqwest::Url::parse(&url)?)
        } else if !RE.is_match(&url) {
            let url = format!("{}{}", default_scheme, url);
            Url(reqwest::Url::parse(&url)?)
        } else {
            Url(reqwest::Url::parse(&url)?)
        })
    }

    pub fn host(&self) -> Option<String> {
        self.0.host().map(|host| host.to_string())
    }
}

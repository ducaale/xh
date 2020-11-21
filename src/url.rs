use regex::Regex;

pub struct Url(pub reqwest::Url);

impl Url {
    pub fn new(url: String, default_scheme: Option<String>) -> Url {
        let default_scheme = default_scheme.unwrap_or("http://".to_string());
        let re = Regex::new("[a-zA-Z]://.+").unwrap();
        if url.starts_with(":") {
            let url = format!("{}{}{}", default_scheme, "localhost", url);
            Url(reqwest::Url::parse(&url).unwrap().into())
        } else if !re.is_match(&url) {
            let url = format!("{}{}", default_scheme, url);
            Url(reqwest::Url::parse(&url).unwrap().into())
        } else {
            Url(reqwest::Url::parse(&url).unwrap())
        }
    }

    pub fn host(&self) -> Option<String> {
        self.0.host().map(|host| host.to_string())
    }
}

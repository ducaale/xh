// Would be slightly cleaner to return a ParseError, but reqwest doesn't
// export that type
use anyhow::Result;
use reqwest::Url;

use crate::regex;

pub fn construct_url(
    url: &str,
    default_scheme: Option<&str>,
    query: Vec<(&str, &str)>,
) -> Result<Url> {
    let default_scheme = default_scheme.unwrap_or("http://");
    let mut url: Url = if url.starts_with(':') {
        format!("{}{}{}", default_scheme, "localhost", url).parse()?
    } else if !regex!("[a-zA-Z]://.+").is_match(&url) {
        format!("{}{}", default_scheme, url).parse()?
    } else {
        url.parse()?
    };
    if !query.is_empty() {
        // If we run this even without adding pairs it adds a `?`, hence
        // the .is_empty() check
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(name, value);
        }
    }
    Ok(url)
}

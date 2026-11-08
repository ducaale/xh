use anyhow::{Context, Result, bail};
use reqwest::header::{
    CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue, TRANSFER_ENCODING,
};
use reqwest::{Method, Url};

use crate::cli::HttpVersion;

#[derive(Debug)]
pub struct HttpFile {
    pub method: Method,
    pub url: Url,
    pub version: HttpVersion,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl HttpFile {
    pub fn parse(mut input: &[u8], scheme: &str) -> Result<Self> {
        fn line<'a>(input: &mut &'a [u8]) -> Result<&'a str> {
            let end = input.iter().position(|&byte| byte == b'\n');
            let (line, rest) = match end {
                Some(end) => (&input[..end], &input[end + 1..]),
                None => (*input, &[][..]),
            };
            *input = rest;
            std::str::from_utf8(line.strip_suffix(b"\r").unwrap_or(line))
                .context("Invalid UTF-8 in HTTP request line or header")
        }

        let request_line = line(&mut input)?;
        let (method, rest) = request_line
            .split_once(' ')
            .context("Invalid HTTP request line")?;
        let (target, version) = rest
            .trim_start()
            .rsplit_once(' ')
            .context("Invalid HTTP request line")?;
        let method = Method::from_bytes(method.as_bytes()).context("Invalid HTTP method")?;
        let version = match version {
            "HTTP/1.0" => HttpVersion::Http10,
            "HTTP/1.1" => HttpVersion::Http11,
            "HTTP/2" | "HTTP/2.0" => HttpVersion::Http2,
            _ => bail!("Unsupported HTTP version: {version}"),
        };
        if !target.starts_with('/')
            || target.contains('#')
            || target.chars().any(|c| c.is_ascii_control())
        {
            bail!("HTTP request target must be a path starting with / without a fragment");
        }

        let mut headers = HeaderMap::new();
        while !input.is_empty() {
            let line = line(&mut input)?;
            if line.is_empty() {
                break;
            }
            let (name, value) = line
                .split_once(':')
                .context("Invalid HTTP header: expected name: value")?;
            let name =
                HeaderName::from_bytes(name.as_bytes()).context("Invalid HTTP header name")?;
            let value = HeaderValue::from_str(value.trim_matches([' ', '\t']))
                .context("Invalid HTTP header value")?;
            headers.append(name, value);
        }

        if headers.get_all(HOST).iter().count() != 1 {
            bail!("HTTP file must contain exactly one Host header");
        }
        let host = headers[HOST].to_str().context("Invalid Host header")?;
        if host.contains(['/', '?', '#', '@', '\\']) {
            bail!("Invalid Host header");
        }
        let mut url = Url::parse(&format!("{scheme}://{host}/")).context("Invalid Host header")?;
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Invalid Host header");
        }
        url.set_path(target.split_once('?').map_or(target, |(path, _)| path));
        url.set_query(target.split_once('?').map(|(_, query)| query));

        for value in headers.get_all(CONTENT_TYPE) {
            if value
                .to_str()?
                .parse::<mime::Mime>()
                .is_ok_and(|mime| mime.type_() == mime::MULTIPART)
            {
                bail!("Multipart requests are not supported in HTTP files");
            }
        }
        if headers.contains_key(TRANSFER_ENCODING) {
            bail!("Transfer-Encoding is not supported in HTTP files; use an unencoded body");
        }
        // Saved requests may have been edited, so let the client recalculate the size.
        headers.remove(CONTENT_LENGTH);

        Ok(Self {
            method,
            url,
            version,
            headers,
            body: input.to_vec(),
        })
    }
}

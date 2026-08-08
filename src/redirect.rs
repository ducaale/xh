use anyhow::Result;
use reqwest::blocking::{Request, Response};
use reqwest::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, LOCATION,
    PROXY_AUTHORIZATION, TRANSFER_ENCODING, WWW_AUTHENTICATE,
};
use reqwest::{Method, StatusCode, Url};

#[cfg(feature = "http-message-signatures")]
use crate::cli::HttpsigOptions;
use crate::middleware::{Context, Middleware};
use crate::utils::{HeaderValueExt, clone_request};

pub struct RedirectFollower {
    max_redirects: usize,
    #[cfg(feature = "http-message-signatures")]
    httpsig: Option<HttpsigOptions>,
    #[cfg(feature = "http-message-signatures")]
    explicit_signature_headers: HeaderMap,
    #[cfg(feature = "http-message-signatures")]
    preserve_signature_headers: bool,
}

impl RedirectFollower {
    pub fn new(
        max_redirects: usize,
        #[cfg(feature = "http-message-signatures")] httpsig: Option<HttpsigOptions>,
        #[cfg(feature = "http-message-signatures")] explicit_signature_headers: HeaderMap,
        #[cfg(feature = "http-message-signatures")] preserve_signature_headers: bool,
    ) -> Self {
        RedirectFollower {
            max_redirects,
            #[cfg(feature = "http-message-signatures")]
            httpsig,
            #[cfg(feature = "http-message-signatures")]
            explicit_signature_headers,
            #[cfg(feature = "http-message-signatures")]
            preserve_signature_headers,
        }
    }
}

impl Middleware for RedirectFollower {
    fn handle(&mut self, mut ctx: Context, mut first_request: Request) -> Result<Response> {
        // This buffers the body in case we need it again later
        // reqwest does *not* do this, it ignores 307/308 with a streaming body
        let mut request = clone_request(&mut first_request)?;
        let mut response = self.next(&mut ctx, first_request)?;
        let mut remaining_redirects = self.max_redirects - 1;

        loop {
            #[cfg(feature = "http-message-signatures")]
            let previous_url = request.url().clone();
            let Some(mut next_request) = get_next_request(
                request,
                &response,
                #[cfg(feature = "http-message-signatures")]
                self.preserve_signature_headers,
            ) else {
                break;
            };
            if remaining_redirects > 0 {
                remaining_redirects -= 1;
            } else {
                return Err(TooManyRedirects {
                    max_redirects: self.max_redirects,
                }
                .into());
            }

            #[cfg(feature = "http-message-signatures")]
            if let Some(httpsig) = &self.httpsig {
                // Sign only same-origin redirects. A cross-origin redirect is
                // a new trust boundary and must not receive the old signature.
                if !is_cross_domain_redirect(next_request.url(), &previous_url) {
                    crate::message_signature::sign_request(
                        &mut next_request,
                        httpsig,
                        &self.explicit_signature_headers,
                    )?;
                }
            }

            log::info!("Following redirect to {}", next_request.url());
            log::trace!("Remaining redirects: {remaining_redirects}");
            log::trace!("{next_request:#?}");
            self.print(&mut ctx, &mut response, &mut next_request)?;
            request = clone_request(&mut next_request)?;
            response = self.next(&mut ctx, next_request)?;
        }

        Ok(response)
    }
}

#[derive(Debug)]
pub(crate) struct TooManyRedirects {
    max_redirects: usize,
}

impl std::fmt::Display for TooManyRedirects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Too many redirects (--max-redirects={})",
            self.max_redirects,
        )
    }
}

impl std::error::Error for TooManyRedirects {}

// See https://github.com/seanmonstar/reqwest/blob/bbeb1ede4e8098481c3de6f2cafb8ecca1db4ede/src/async_impl/client.rs#L1500-L1607
fn get_next_request(
    mut request: Request,
    response: &Response,
    #[cfg(feature = "http-message-signatures")] preserve_signature_headers: bool,
) -> Option<Request> {
    #[cfg(not(feature = "http-message-signatures"))]
    let preserve_signature_headers = false;

    let get_next_url = |request: &Request| {
        let location = response.headers().get(LOCATION)?;
        let url = location
            .to_utf8_str()
            .ok()
            .and_then(|location| request.url().join(location).ok());
        if url.is_none() {
            log::warn!("Redirect to invalid URL: {location:?}");
        }
        url
    };

    match response.status() {
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER => {
            let next_url = get_next_url(&request)?;
            log::trace!("Preparing redirect to {next_url}");
            let prev_url = request.url();
            let cross_domain = is_cross_domain_redirect(&next_url, prev_url);
            if cross_domain {
                remove_sensitive_headers(request.headers_mut());
            }
            if !preserve_signature_headers {
                remove_signature_headers(request.headers_mut());
            }
            remove_content_headers(request.headers_mut());
            *request.url_mut() = next_url;
            *request.body_mut() = None;
            *request.method_mut() = match *request.method() {
                Method::GET => Method::GET,
                Method::HEAD => Method::HEAD,
                _ => Method::GET,
            };
            Some(request)
        }
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => {
            let next_url = get_next_url(&request)?;
            log::trace!("Preparing redirect to {next_url}");
            let prev_url = request.url();
            let cross_domain = is_cross_domain_redirect(&next_url, prev_url);
            if cross_domain {
                remove_sensitive_headers(request.headers_mut());
            }
            if !preserve_signature_headers {
                remove_signature_headers(request.headers_mut());
            }
            *request.url_mut() = next_url;
            Some(request)
        }
        _ => None,
    }
}

// See https://github.com/seanmonstar/reqwest/blob/bbeb1ede4e8098481c3de6f2cafb8ecca1db4ede/src/redirect.rs#L234-L246
fn is_cross_domain_redirect(next: &Url, previous: &Url) -> bool {
    next.scheme() != previous.scheme()
        || next.host_str() != previous.host_str()
        || next.port_or_known_default() != previous.port_or_known_default()
}

// See https://github.com/seanmonstar/reqwest/blob/bbeb1ede4e8098481c3de6f2cafb8ecca1db4ede/src/redirect.rs#L234-L246
fn remove_sensitive_headers(headers: &mut HeaderMap) {
    log::debug!("Removing sensitive headers for cross-domain redirect");
    headers.remove(AUTHORIZATION);
    headers.remove(COOKIE);
    headers.remove("cookie2");
    headers.remove(PROXY_AUTHORIZATION);
    headers.remove(WWW_AUTHENTICATE);
}

// See https://github.com/seanmonstar/reqwest/blob/bbeb1ede4e8098481c3de6f2cafb8ecca1db4ede/src/async_impl/client.rs#L1503-L1510
fn remove_content_headers(headers: &mut HeaderMap) {
    log::debug!("Removing content headers for redirect that strips body");
    headers.remove(TRANSFER_ENCODING);
    headers.remove(CONTENT_ENCODING);
    headers.remove(CONTENT_TYPE);
    headers.remove(CONTENT_LENGTH);
    headers.remove("content-digest");
}

fn remove_signature_headers(headers: &mut HeaderMap) {
    log::debug!("Removing signature headers before redirect");
    headers.remove("signature");
    headers.remove("signature-input");
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn remove_content_headers_removes_content_digest() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("1"));
        headers.insert("content-digest", HeaderValue::from_static("sha-256=:abc=:"));

        remove_content_headers(&mut headers);

        assert!(!headers.contains_key(CONTENT_LENGTH));
        assert!(!headers.contains_key("content-digest"));
    }

    #[test]
    fn scheme_changes_are_cross_origin_redirects() {
        let previous: Url = "http://example.com/path".parse().unwrap();
        let next: Url = "https://example.com/path".parse().unwrap();

        assert!(is_cross_domain_redirect(&next, &previous));
    }
}

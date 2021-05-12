use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::blocking::{Request, Response};
use reqwest::header::{
    HeaderMap, AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION,
    PROXY_AUTHORIZATION, TRANSFER_ENCODING, WWW_AUTHENTICATE,
};
use reqwest::{Method, StatusCode, Url};

#[derive(Debug)]
enum ClonedRequest {
    Partial(Request),
    Full(Request),
}

impl ClonedRequest {
    fn inner(self) -> Request {
        match self {
            Self::Partial(request) | Self::Full(request) => request,
        }
    }
}

fn clone_request(request: &Request) -> ClonedRequest {
    let mut cloned_request = Request::new(request.method().clone(), request.url().clone());
    *cloned_request.timeout_mut() = request.timeout().cloned();
    *cloned_request.headers_mut() = request.headers().clone();

    match request.body().map(|b| b.as_bytes()) {
        Some(Some(body)) => {
            *cloned_request.body_mut() = Some(body.to_owned().into());
            ClonedRequest::Full(cloned_request)
        }
        Some(None) => {
            for header in &[
                TRANSFER_ENCODING,
                CONTENT_ENCODING,
                CONTENT_TYPE,
                CONTENT_LENGTH,
            ] {
                cloned_request.headers_mut().remove(header);
            }
            ClonedRequest::Partial(cloned_request)
        }
        None => ClonedRequest::Full(cloned_request),
    }
}

fn next_request(request: &ClonedRequest, response: &Response) -> Option<Request> {
    let get_next_url = |request: &Request| {
        response
            .headers()
            .get(LOCATION)
            .and_then(|location| location.to_str().ok())
            .and_then(|location| request.url().join(location).ok())
    };

    match response.status() {
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER => {
            match request {
                ClonedRequest::Full(request) | ClonedRequest::Partial(request) => {
                    let mut request = clone_request(&request).inner();
                    let next_url = get_next_url(&request)?;
                    let prev_url = request.url().clone();
                    remove_sensitive_headers(request.headers_mut(), &next_url, &prev_url);
                    *request.url_mut() = next_url;
                    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
                        *request.method_mut() = Method::GET;
                    }
                    Some(request)
                }
            }
        }
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => match request {
            ClonedRequest::Full(request) => {
                let mut request = clone_request(&request).inner();
                let next_url = get_next_url(&request)?;
                let prev_url = request.url().clone();
                remove_sensitive_headers(request.headers_mut(), &next_url, &prev_url);
                *request.url_mut() = next_url;
                Some(request)
            }
            ClonedRequest::Partial(..) => None,
        },
        _ => None,
    }
}

// Copied from https://github.com/seanmonstar/reqwest/blob/bbeb1ede4e8098481c3de6f2cafb8ecca1db4ede/src/redirect.rs#L234
// with slight modifications
fn remove_sensitive_headers(headers: &mut HeaderMap, next: &Url, previous: &Url) {
    let cross_host = next.host_str() != previous.host_str()
        || next.port_or_known_default() != previous.port_or_known_default();
    if cross_host {
        headers.remove(AUTHORIZATION);
        headers.remove(COOKIE);
        headers.remove("cookie2");
        headers.remove(PROXY_AUTHORIZATION);
        headers.remove(WWW_AUTHENTICATE);
    }
}

pub struct RedirectFollower<'a, T>
where
    T: FnMut(Response, &Request) -> Result<()>,
{
    client: &'a Client,
    max_redirects: usize,
    callback: Option<T>,
}

impl<'a, T> RedirectFollower<'a, T>
where
    T: FnMut(Response, &Request) -> Result<()>,
{
    pub fn new(client: &'a Client, max_redirects: usize) -> Self {
        RedirectFollower {
            client,
            max_redirects,
            callback: None,
        }
    }

    pub fn on_redirect(&mut self, callback: T) {
        self.callback = Some(callback);
    }

    pub fn execute(&mut self, request: Request) -> Result<Response> {
        let mut cloned_request = clone_request(&request);
        let mut response = self.client.execute(request)?;
        while let Some(next_request) = next_request(&cloned_request, &response) {
            if let Some(ref mut callback) = self.callback {
                callback(response, &next_request)?;
            }
            cloned_request = clone_request(&next_request);
            response = self.client.execute(next_request)?;
        }
        Ok(response)
    }
}

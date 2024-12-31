use anyhow::Result;
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderValue, HOST};
use std::path::PathBuf;

pub struct UnixClient {
    rt: tokio::runtime::Runtime,
    socket_path: PathBuf,
}

impl UnixClient {
    pub fn new(socket_path: PathBuf) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { rt, socket_path }
    }

    pub fn execute(&self, request: Request) -> Result<Response> {
        self.rt.block_on(async {
            // TODO: Add support for Windows named pipes by replacing UnixStream with namedPipeClient.
            // See https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ClientOptions.html#method.open
            let stream = tokio::net::UnixStream::connect(&self.socket_path).await?;

            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .title_case_headers(true)
                .handshake(hyper_util::rt::TokioIo::new(stream))
                .await?;

            tokio::task::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("Connection failed: {:?}", err);
                }
            });

            // TODO: don't ignore value from --timeout option
            let http_request = into_async_request(request)?;
            let response = sender.send_request(http_request).await?;

            Ok(Response::from(response.map(reqwest::Body::wrap)))
        })
    }
}

fn into_async_request(mut request: Request) -> Result<http::Request<reqwest::Body>> {
    let mut http_request = http::Request::builder()
        .version(request.version())
        .method(request.method())
        .uri(request.url().as_str())
        .body(reqwest::Body::default())?;

    *http_request.headers_mut() = request.headers_mut().clone();

    if let Some(host) = request.url().host_str() {
        http_request.headers_mut().entry(HOST).or_insert_with(|| {
            if let Some(port) = request.url().port() {
                HeaderValue::from_str(&format!("{}:{}", host, port))
            } else {
                HeaderValue::from_str(host)
            }
            .expect("hostname should already be validated/parsed")
        });
    }

    if let Some(body) = request.body_mut().as_mut() {
        *http_request.body_mut() = reqwest::Body::from(body.buffer()?.to_owned());
    }

    Ok(http_request)
}

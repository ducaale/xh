use anyhow::Result;
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderValue, HOST};
use std::path::PathBuf;
use std::time::Instant;

use crate::middleware::{Context, Middleware, ResponseMeta};

pub struct UnixSocket {
    rt: tokio::runtime::Runtime,
    socket_path: PathBuf,
}

impl UnixSocket {
    pub fn new(socket_path: PathBuf) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { rt, socket_path }
    }

    pub fn execute(&self, request: Request) -> Result<Response> {
        self.rt.block_on(async {
            // TODO: Support named pipes by replacing tokio::net::UnixStream::connect(..) with:
            //
            // use std::time::Duration;
            // use tokio::net::windows::named_pipe;
            // use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
            //
            // let stream = loop {
            //     match named_pipe::ClientOptions::new().open(r"\\.\pipe\docker_engine") {
            //         Ok(client) => break client,
            //         Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => (),
            //         Err(e) => return Err(e)?,
            //     }
            //
            //     tokio::time::sleep(Duration::from_millis(50)).await;
            // };
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

impl Middleware for UnixSocket {
    fn handle(&mut self, mut _ctx: Context, request: Request) -> Result<Response> {
        let starting_time = Instant::now();
        let mut response = self.execute(request)?;
        response.extensions_mut().insert(ResponseMeta {
            request_duration: starting_time.elapsed(),
            content_download_duration: None,
        });
        Ok(response)
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

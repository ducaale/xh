use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::{anyhow, Result};
use pin_project_lite::pin_project;
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderValue, HOST};
use tokio::time::Sleep;

pub struct UnixClient {
    rt: tokio::runtime::Runtime,
    socket_path: PathBuf,
    timeout: Option<Duration>,
}

impl UnixClient {
    pub fn new(socket_path: PathBuf, timeout: Option<Duration>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self {
            rt,
            socket_path,
            timeout,
        }
    }

    pub fn execute(&self, request: Request) -> Result<Response> {
        self.rt.block_on(async {
            // TODO: Add support for Windows named pipes by replacing UnixStream with namedPipeClient.
            // See https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ClientOptions.html#method.open

            // TODO: connection timeout??
            let stream = tokio::net::UnixStream::connect(&self.socket_path).await?;

            // TODO: connection timeout
            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .title_case_headers(true)
                .handshake(hyper_util::rt::TokioIo::new(stream))
                .await?;

            tokio::task::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("Connection failed: {:?}", err);
                }
            });

            let http_request = into_async_request(request)?;

            let response = if let Some(timeout) = self.timeout {
                tokio::time::timeout(timeout, sender.send_request(http_request))
                    .await
                    .map_err(|_| anyhow!(TimeoutError))?
            } else {
                sender.send_request(http_request).await
            }?;

            Ok(Response::from(response.map(|body| {
                if let Some(timeout) = self.timeout {
                    reqwest::Body::wrap(ReadTimeoutBody::new(body, timeout))
                } else {
                    reqwest::Body::wrap(body)
                }
            })))
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

#[derive(Debug, Clone)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

// Copied from https://github.com/seanmonstar/reqwest/blob/8b8fdd2552ad645c7e9dd494930b3e95e2aedef2/src/async_impl/body.rs#L347
// with some slight tweaks
pin_project! {
    pub(crate) struct ReadTimeoutBody<B> {
        #[pin]
        inner: B,
        #[pin]
        sleep: Option<Sleep>,
        timeout: Duration,
    }
}

impl<B> ReadTimeoutBody<B> {
    fn new(body: B, timeout: Duration) -> ReadTimeoutBody<B> {
        ReadTimeoutBody {
            inner: body,
            sleep: None,
            timeout,
        }
    }
}

impl<B> hyper::body::Body for ReadTimeoutBody<B>
where
    B: hyper::body::Body,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Data = B::Data;
    type Error = anyhow::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let mut this = self.project();

        // Start the `Sleep` if not active.
        let sleep_pinned = if let Some(some) = this.sleep.as_mut().as_pin_mut() {
            some
        } else {
            this.sleep.set(Some(tokio::time::sleep(*this.timeout)));
            this.sleep.as_mut().as_pin_mut().unwrap()
        };

        // Error if the timeout has expired.
        if let Poll::Ready(()) = sleep_pinned.poll(cx) {
            return Poll::Ready(Some(Err(anyhow!(TimeoutError))));
        }

        let item = futures_core::ready!(this.inner.poll_frame(cx))
            .map(|opt_chunk| opt_chunk.map_err(|e| anyhow!(e.into())));
        // a ready frame means timeout is reset
        this.sleep.set(None);
        Poll::Ready(item)
    }

    #[inline]
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
}

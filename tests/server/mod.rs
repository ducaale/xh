// Copied from https://raw.githubusercontent.com/seanmonstar/reqwest/v0.12.0/tests/support/server.rs
// with some slight tweaks
use std::convert::Infallible;
use std::future::Future;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::runtime;
use tokio::sync::oneshot;

type Body = Full<Bytes>;
type Builder = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

enum Listener {
    TcpListener(tokio::net::TcpListener),
    #[cfg(unix)]
    UnixListener(tempfile::NamedTempFile<tokio::net::UnixListener>),
}

pub struct Server {
    listener: Arc<Listener>,
    panic_rx: std_mpsc::Receiver<()>,
    successful_hits: Arc<Mutex<u8>>,
    total_hits: Arc<Mutex<u8>>,
    no_hit_checks: bool,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl Server {
    pub fn base_url(&self) -> String {
        match &*self.listener {
            Listener::TcpListener(l) => format!("http://{}", l.local_addr().unwrap()),
            #[cfg(unix)]
            _ => panic!("no base_url for unix server"),
        }
    }

    pub fn url(&self, path: &str) -> String {
        match &*self.listener {
            Listener::TcpListener(l) => format!("http://{}{}", l.local_addr().unwrap(), path),
            #[cfg(unix)]
            _ => panic!("no url for unix server"),
        }
    }

    pub fn host(&self) -> String {
        match &*self.listener {
            Listener::TcpListener(_) => String::from("127.0.0.1"),
            #[cfg(unix)]
            _ => panic!("no host for unix server"),
        }
    }

    #[cfg(unix)]
    pub fn socket_path(&self) -> std::path::PathBuf {
        match &*self.listener {
            Listener::UnixListener(l) => l
                .as_file()
                .local_addr()
                .unwrap()
                .as_pathname()
                .unwrap()
                .to_path_buf(),
            _ => panic!("no socket_path for tcp server"),
        }
    }

    pub fn port(&self) -> u16 {
        match &*self.listener {
            Listener::TcpListener(l) => l.local_addr().unwrap().port(),
            #[cfg(unix)]
            _ => panic!("no port for unix server"),
        }
    }

    pub fn assert_hits(&self, hits: u8) {
        assert_eq!(*self.successful_hits.lock().unwrap(), hits);
    }

    pub fn disable_hit_checks(&mut self) {
        self.no_hit_checks = true;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if !std::thread::panicking() && !self.no_hit_checks {
            let total_hits = *self.total_hits.lock().unwrap();
            let successful_hits = *self.successful_hits.lock().unwrap();
            let failed_hits = total_hits - successful_hits;
            assert!(total_hits > 0, "test server exited without being called");
            assert_eq!(
                failed_hits, 0,
                "numbers of panicked or in-progress requests: {failed_hits}"
            );
        }

        if !std::thread::panicking() {
            self.panic_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("test server should not panic");
        }
    }
}

// http() is generic, http_inner() is not.
// A generic function has to be compiled for every single type you use it with.
// And every closure counts as a different type.
// By making only http() generic a rebuild of the tests take 3-10 times less long.

pub fn http<F, Fut>(func: F) -> Server
where
    F: Fn(Request<hyper::body::Incoming>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response<Body>> + Send + 'static,
{
    http_inner(Arc::new(move |req| Box::new(Box::pin(func(req)))), false)
}

#[cfg(unix)]
pub fn http_unix<F, Fut>(func: F) -> Server
where
    F: Fn(Request<hyper::body::Incoming>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response<Body>> + Send + 'static,
{
    http_inner(Arc::new(move |req| Box::new(Box::pin(func(req)))), true)
}

type Serv = dyn Fn(Request<hyper::body::Incoming>) -> Box<ServFut> + Send + Sync;
type ServFut = dyn Future<Output = Response<Body>> + Send + Unpin;

fn http_inner(func: Arc<Serv>, use_unix_socket: bool) -> Server {
    // Spawn new runtime in thread to prevent reactor execution context conflict
    thread::spawn(move || {
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("new rt");
        let successful_hits = Arc::new(Mutex::new(0));
        let total_hits = Arc::new(Mutex::new(0));

        let listener = Arc::new(rt.block_on(async move {
            if use_unix_socket {
                #[cfg(not(unix))]
                {
                    panic!("unix server not supported")
                }
                #[cfg(unix)]
                {
                    tempfile::Builder::new()
                        .make(|path| tokio::net::UnixListener::bind(path))
                        .map(Listener::UnixListener)
                        .unwrap()
                }
            } else {
                tokio::net::TcpListener::bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
                    .await
                    .map(Listener::TcpListener)
                    .unwrap()
            }
        }));

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (panic_tx, panic_rx) = std_mpsc::channel();
        let thread_name = format!(
            "test({})-support-server",
            thread::current().name().unwrap_or("<unknown>")
        );

        {
            let successful_hits = successful_hits.clone();
            let total_hits = total_hits.clone();
            let listener = listener.clone();
            thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    let task = rt.spawn(async move {
                        let builder = Builder::new(hyper_util::rt::TokioExecutor::new());
                        loop {
                            let svc = {
                                let func = func.clone();
                                let successful_hits = successful_hits.clone();
                                let total_hits = total_hits.clone();

                                service_fn(move |req| {
                                    let successful_hits = successful_hits.clone();
                                    let total_hits = total_hits.clone();
                                    let fut = func(req);
                                    async move {
                                        *total_hits.lock().unwrap() += 1;
                                        let res = fut.await;
                                        *successful_hits.lock().unwrap() += 1;
                                        Ok::<_, Infallible>(res)
                                    }
                                })
                            };

                            let builder = builder.clone();

                            match &*listener {
                                Listener::TcpListener(listener) => {
                                    let (io, _) = listener.accept().await.unwrap();
                                    tokio::spawn(async move {
                                        let _ =
                                            builder.serve_connection(TokioIo::new(io), svc).await;
                                    });
                                }
                                #[cfg(unix)]
                                Listener::UnixListener(listener) => {
                                    let (io, _) = listener.as_file().accept().await.unwrap();
                                    tokio::spawn(async move {
                                        let _ =
                                            builder.serve_connection(TokioIo::new(io), svc).await;
                                    });
                                }
                            };
                        }
                    });
                    let _ = rt.block_on(shutdown_rx);
                    task.abort();
                    let _ = panic_tx.send(());
                })
                .expect("thread spawn");
        }
        Server {
            listener,
            panic_rx,
            shutdown_tx: Some(shutdown_tx),
            successful_hits,
            total_hits,
            no_hit_checks: false,
        }
    })
    .join()
    .unwrap()
}

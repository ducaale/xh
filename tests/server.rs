// Copied from https://github.com/seanmonstar/reqwest/blob/ab49de875ec2326abf25f52f54b249a28e43b69c/tests/support/server.rs
// with some slight tweaks
use std::convert::Infallible;
use std::future::Future;
use std::net;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hyper::service::{make_service_fn, service_fn};
use tokio::runtime;
use tokio::sync::oneshot;

pub struct Server {
    addr: net::SocketAddr,
    call_counter: Arc<Mutex<u8>>,
    panic_rx: std_mpsc::Receiver<()>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl Server {
    pub fn addr(&self) -> String {
        self.addr.to_string()
    }

    pub fn assert_called_once(&self) {
        assert_eq!(*self.call_counter.lock().unwrap(), 1);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if !::std::thread::panicking() {
            self.panic_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("test server should not panic");
        }
    }
}

pub fn http<F, Fut>(func: F) -> Server
where
    F: Fn(http::Request<hyper::Body>) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = http::Response<hyper::Body>> + Send + 'static,
{
    //Spawn new runtime in thread to prevent reactor execution context conflict
    thread::spawn(move || {
        let call_counter = Arc::new(Mutex::new(0));
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("new rt");
        let srv = {
            let call_counter = call_counter.clone();
            rt.block_on(async move {
                let make_service = make_service_fn(move |_| {
                    let func = func.clone();
                    let call_counter = call_counter.clone();
                    async move {
                        Ok::<_, Infallible>(service_fn(move |req| {
                            let fut = func(req);
                            let call_counter = call_counter.clone();
                            async move {
                                let res = fut.await;
                                let mut num = call_counter.lock().unwrap();
                                *num += 1;
                                Ok::<_, Infallible>(res)
                            }
                        }))
                    }
                });
                hyper::Server::bind(&([127, 0, 0, 1], 0).into()).serve(make_service)
            })
        };

        let addr = srv.local_addr();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let srv = srv.with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });

        let (panic_tx, panic_rx) = std_mpsc::channel();
        let tname = format!(
            "test({})-support-server",
            thread::current().name().unwrap_or("<unknown>")
        );
        thread::Builder::new()
            .name(tname)
            .spawn(move || {
                rt.block_on(srv).unwrap();
                let _ = panic_tx.send(());
            })
            .expect("thread spawn");

        Server {
            addr,
            call_counter,
            panic_rx,
            shutdown_tx: Some(shutdown_tx),
        }
    })
    .join()
    .unwrap()
}

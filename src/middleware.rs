use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::blocking::{Client, Request, Response};

#[derive(Clone)]
pub struct ResponseMeta {
    pub request_duration: Duration,
    pub content_download_duration: Option<Duration>,
}

pub trait ResponseExt {
    fn meta(&self) -> &ResponseMeta;
    fn meta_mut(&mut self) -> &mut ResponseMeta;
}

impl ResponseExt for Response {
    fn meta(&self) -> &ResponseMeta {
        self.extensions().get::<ResponseMeta>().unwrap()
    }

    fn meta_mut(&mut self) -> &mut ResponseMeta {
        self.extensions_mut().get_mut::<ResponseMeta>().unwrap()
    }
}

type Printer<'a> = &'a mut (dyn FnMut(&mut Response, &mut Request) -> Result<()> + 'a);

pub struct Context<'a, 'b, 'c> {
    client: Client,
    printer: Printer<'c>,
    middlewares: &'a mut [Box<dyn Middleware + 'b>],
}

impl<'a, 'b, 'c> Context<'a, 'b, 'c> {
    fn new(
        client: Client,
        printer: Printer<'c>,
        middlewares: &'a mut [Box<dyn Middleware + 'b>],
    ) -> Self {
        Context {
            client,
            printer,
            middlewares,
        }
    }

    fn execute(&mut self, request: Request) -> Result<Response> {
        match self.middlewares {
            [] => {
                let starting_time = Instant::now();
                let mut response = self.client.execute(request)?;
                response.extensions_mut().insert(ResponseMeta {
                    request_duration: starting_time.elapsed(),
                    content_download_duration: None,
                });
                Ok(response)
            }
            [ref mut head, tail @ ..] => head.handle(
                Context::new(self.client.clone(), self.printer, tail),
                request,
            ),
        }
    }
}

pub trait Middleware {
    fn handle(&mut self, ctx: Context, request: Request) -> Result<Response>;

    fn next(&self, ctx: &mut Context, request: Request) -> Result<Response> {
        ctx.execute(request)
    }

    fn print(
        &self,
        ctx: &mut Context,
        response: &mut Response,
        request: &mut Request,
    ) -> Result<()> {
        (ctx.printer)(response, request)?;
        Ok(())
    }
}

pub struct ClientWithMiddleware<'a> {
    client: Client,
    middlewares: Vec<Box<dyn Middleware + 'a>>,
}

impl<'a> ClientWithMiddleware<'a> {
    pub fn new(client: Client) -> Self {
        ClientWithMiddleware {
            client,
            middlewares: vec![],
        }
    }

    pub fn with(mut self, middleware: impl Middleware + 'a) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }

    pub fn execute<'b, T>(&mut self, request: Request, mut printer: T) -> Result<Response>
    where
        T: FnMut(&mut Response, &mut Request) -> Result<()> + 'b,
    {
        let mut ctx = Context::new(self.client.clone(), &mut printer, &mut self.middlewares[..]);
        ctx.execute(request)
    }
}

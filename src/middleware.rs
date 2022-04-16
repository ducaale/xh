use std::time::Instant;

use anyhow::Result;
use reqwest::blocking::{Client, Request, Response};

#[derive(Clone)]
pub struct ResponseMeta {
    pub starting_time: Instant,
}

impl ResponseMeta {
    fn new(starting_time: Instant) -> Self {
        ResponseMeta { starting_time }
    }
}

pub trait ResponseExt {
    fn metadata(&self) -> &ResponseMeta;
}

impl ResponseExt for Response {
    fn metadata(&self) -> &ResponseMeta {
        self.extensions().get::<ResponseMeta>().unwrap()
    }
}

pub struct Context<'a, 'b> {
    client: &'a Client,
    printer: Option<&'a mut (dyn FnMut(&mut Response, &mut Request) -> Result<()> + 'b)>,
    middlewares: &'a mut [Box<dyn Middleware + 'b>],
}

impl<'a, 'b> Context<'a, 'b> {
    fn new(
        client: &'a Client,
        printer: Option<&'a mut (dyn FnMut(&mut Response, &mut Request) -> Result<()> + 'b)>,
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
                response
                    .extensions_mut()
                    .insert(ResponseMeta::new(starting_time));
                Ok(response)
            }
            [ref mut head, tail @ ..] => head.handle(
                #[allow(clippy::needless_option_as_deref)]
                Context::new(self.client, self.printer.as_deref_mut(), tail),
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
        if let Some(ref mut printer) = ctx.printer {
            printer(response, request)?;
        }

        Ok(())
    }
}

pub struct ClientWithMiddleware<'a, T>
where
    T: FnMut(&mut Response, &mut Request) -> Result<()>,
{
    client: &'a Client,
    printer: Option<T>,
    middlewares: Vec<Box<dyn Middleware + 'a>>,
}

impl<'a, T> ClientWithMiddleware<'a, T>
where
    T: FnMut(&mut Response, &mut Request) -> Result<()> + 'a,
{
    pub fn new(client: &'a Client) -> Self {
        ClientWithMiddleware {
            client,
            printer: None,
            middlewares: vec![],
        }
    }

    pub fn with_printer(mut self, printer: T) -> Self {
        self.printer = Some(printer);
        self
    }

    pub fn with(mut self, middleware: impl Middleware + 'a) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }

    pub fn execute(&mut self, request: Request) -> Result<Response> {
        let mut ctx = Context::new(
            self.client,
            self.printer.as_mut().map(|p| p as _),
            &mut self.middlewares[..],
        );
        ctx.execute(request)
    }
}

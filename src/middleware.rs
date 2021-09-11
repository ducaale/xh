use anyhow::Result;
use reqwest::blocking::{Client, Request, Response};

pub struct Next<'a, 'b> {
    client: &'a Client,
    middlewares: &'a mut [Box<dyn Middleware + 'b>],
}

impl<'a, 'b> Next<'a, 'b> {
    fn new(client: &'a Client, middlewares: &'a mut [Box<dyn Middleware + 'b>]) -> Self {
        Next {
            client,
            middlewares,
        }
    }

    pub fn run(&mut self, request: Request) -> Result<Response> {
        match self.middlewares {
            [] => Ok(self.client.execute(request)?),
            [ref mut head, tail @ ..] => head.handle(request, Next::new(self.client, tail)),
        }
    }
}

pub trait Middleware {
    fn handle(&mut self, request: Request, next: Next) -> Result<Response>;
}

pub struct ClientWithMiddleware<'a> {
    client: &'a Client,
    middlewares: Vec<Box<dyn Middleware + 'a>>,
}

impl<'a> ClientWithMiddleware<'a> {
    pub fn new(client: &'a Client) -> Self {
        ClientWithMiddleware {
            client,
            middlewares: vec![],
        }
    }

    pub fn with(mut self, middleware: impl Middleware + 'a) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }

    pub fn execute(&mut self, request: Request) -> Result<Response> {
        let mut next = Next::new(self.client, &mut self.middlewares[..]);
        next.run(request)
    }
}

use anyhow::Result;
use reqwest::blocking::{Client, Request, Response};

// TODO: come up with a more suitable name than "Next"
// maybe "Handler"??
pub struct Next<'a, 'b> {
    client: &'a Client,
    pub printer: Option<&'a mut (dyn FnMut(Response, &mut Request) -> Result<()> + 'b)>,
    middlewares: &'a mut [Box<dyn Middleware + 'b>],
}

impl<'a, 'b> Next<'a, 'b> {
    fn new(
        client: &'a Client,
        printer: Option<&'a mut (dyn FnMut(Response, &mut Request) -> Result<()> + 'b)>,
        middlewares: &'a mut [Box<dyn Middleware + 'b>],
    ) -> Self {
        Next {
            client,
            printer,
            middlewares,
        }
    }

    pub fn run(&mut self, request: Request) -> Result<Response> {
        match self.middlewares {
            [] => Ok(self.client.execute(request)?),
            [ref mut head, tail @ ..] => head.handle(
                request,
                Next::new(self.client, self.printer.as_deref_mut(), tail),
            ),
        }
    }
}

pub trait Middleware {
    fn handle(&mut self, request: Request, next: Next) -> Result<Response>;
}

pub struct ClientWithMiddleware<'a, T>
where
    T: FnMut(Response, &mut Request) -> Result<()>,
{
    client: &'a Client,
    printer: Option<T>,
    middlewares: Vec<Box<dyn Middleware + 'a>>,
}

impl<'a, T: 'a> ClientWithMiddleware<'a, T>
where
    T: FnMut(Response, &mut Request) -> Result<()>,
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
        let mut next = Next::new(
            self.client,
            self.printer
                .as_mut()
                .map(|p| p as &mut dyn FnMut(Response, &mut Request) -> Result<()>),
            &mut self.middlewares[..],
        );
        next.run(request)
    }
}

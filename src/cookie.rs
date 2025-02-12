use std::sync::Arc;

use anyhow::Result;
use reqwest::{
    blocking::{Request, Response},
    cookie::CookieStore,
    header,
};

use crate::middleware::{Context, Middleware};

pub struct CookieMiddleware<T>(Arc<T>);

impl<T> CookieMiddleware<T> {
    pub fn new(cookie_jar: Arc<T>) -> Self {
        CookieMiddleware(cookie_jar)
    }
}

impl<T: CookieStore> Middleware for CookieMiddleware<T> {
    fn handle(&mut self, mut ctx: Context, mut request: Request) -> Result<Response> {
        let url = request.url().clone();

        if let Some(header) = self.0.cookies(&url) {
            request
                .headers_mut()
                .entry(header::COOKIE)
                .or_insert(header);
        }

        let response = self.next(&mut ctx, request)?;

        let mut cookies = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .peekable();
        if cookies.peek().is_some() {
            self.0.set_cookies(&mut cookies, &url);
        }

        Ok(response)
    }
}

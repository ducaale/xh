//! This is a vendored version of reqwest_cookie_store: https://github.com/pfernie/reqwest_cookie_store
//!
//! It has been slightly modified for xh's use case. It may be unvendored
//! if reqwest_cookie_store is updated to use reqwest@0.13
//!
//! Copyright 2017 pfernie, see LICENSE-REQWEST_COOKIE_STORE.txt.

use std::sync::{Mutex, MutexGuard, PoisonError};

pub use cookie_store::{CookieStore, RawCookie, RawCookieParseError};
use reqwest::header::HeaderValue;
use url;

fn set_cookies(
    cookie_store: &mut CookieStore,
    cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
    url: &url::Url,
) {
    let cookies = cookie_headers.filter_map(|val| {
        std::str::from_utf8(val.as_bytes())
            .map_err(RawCookieParseError::from)
            .and_then(RawCookie::parse)
            .map(|c| c.into_owned())
            .ok()
    });
    cookie_store.store_response_cookies(cookies, url);
}

fn cookies(cookie_store: &CookieStore, url: &url::Url) -> Option<HeaderValue> {
    let s = cookie_store
        .get_request_values(url)
        .map(|(name, value)| format!("{}={}", name, value))
        .collect::<Vec<_>>()
        .join("; ");

    if s.is_empty() {
        return None;
    }

    HeaderValue::from_str(&s).ok()
}

/// A [`cookie_store::CookieStore`] wrapped internally by a [`std::sync::Mutex`], suitable for use in
/// async/concurrent contexts.
#[derive(Debug)]
pub struct CookieStoreMutex(Mutex<CookieStore>);

impl Default for CookieStoreMutex {
    /// Create a new, empty [`CookieStoreMutex`]
    fn default() -> Self {
        CookieStoreMutex::new(CookieStore::default())
    }
}

impl CookieStoreMutex {
    /// Create a new [`CookieStoreMutex`] from an existing [`cookie_store::CookieStore`].
    pub const fn new(cookie_store: CookieStore) -> CookieStoreMutex {
        CookieStoreMutex(Mutex::new(cookie_store))
    }

    /// Lock and get a handle to the contained [`cookie_store::CookieStore`].
    pub fn lock(
        &self,
    ) -> Result<MutexGuard<'_, CookieStore>, PoisonError<MutexGuard<'_, CookieStore>>> {
        self.0.lock()
    }
}

impl reqwest::cookie::CookieStore for CookieStoreMutex {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let mut store = self.0.lock().unwrap();
        set_cookies(&mut store, cookie_headers, url);
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let store = self.0.lock().unwrap();
        cookies(&store, url)
    }
}

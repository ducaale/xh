use std::borrow::Cow;
use std::env::var_os;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

use anyhow::Result;
use reqwest::blocking::{Request, Response};
use reqwest::header::HeaderValue;
use url::Url;

pub fn unescape(text: &str, special_chars: &'static str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) if special_chars.contains(next) => {
                    // Escape this character
                    out.push(next);
                }
                Some(next) => {
                    // Do not escape this character, treat backslash
                    // as ordinary character
                    out.push(ch);
                    out.push(next);
                }
                None => {
                    out.push(ch);
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn clone_request(request: &mut Request) -> Result<Request> {
    if let Some(b) = request.body_mut().as_mut() {
        b.buffer()?;
    }
    // This doesn't copy the contents of the buffer, cloning requests is cheap
    // https://docs.rs/bytes/1.0.1/bytes/struct.Bytes.html
    Ok(request.try_clone().unwrap()) // guaranteed to not fail if body is already buffered
}

/// Whether to make some things more deterministic for the benefit of tests
pub fn test_mode() -> bool {
    // In integration tests the binary isn't compiled with cfg(test), so we
    // use an environment variable.
    // This isn't called very often currently but we could cache it using an
    // atomic integer.
    cfg!(test) || var_os("XH_TEST_MODE").is_some()
}

/// Whether to behave as if stdin and stdout are terminals
pub fn test_pretend_term() -> bool {
    var_os("XH_TEST_MODE_TERM").is_some()
}

pub fn test_default_color() -> bool {
    var_os("XH_TEST_MODE_COLOR").is_some()
}

#[cfg(test)]
pub fn random_string() -> String {
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
}

pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XH_CONFIG_DIR") {
        return Some(dir.into());
    }

    if cfg!(target_os = "macos") {
        // On macOS dirs returns `~/Library/Application Support`.
        // ~/.config is more usual so we switched to that. But first we check for
        // the legacy location.
        let legacy_config_dir = dirs::config_dir()?.join("xh");
        let config_home = match var_os("XDG_CONFIG_HOME") {
            Some(dir) => dir.into(),
            None => dirs::home_dir()?.join(".config"),
        };
        let new_config_dir = config_home.join("xh");
        if legacy_config_dir.exists() && !new_config_dir.exists() {
            Some(legacy_config_dir)
        } else {
            Some(new_config_dir)
        }
    } else {
        Some(dirs::config_dir()?.join("xh"))
    }
}

pub fn get_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("XH_TEST_MODE_WIN_HOME_DIR") {
        return Some(PathBuf::from(path));
    }

    dirs::home_dir()
}

/// Perform simple tilde expansion if `dirs::home_dir()` is `Some(path)`.
///
/// Note that prefixed tilde e.g `~foo` is ignored.
///
/// See https://www.gnu.org/software/bash/manual/html_node/Tilde-Expansion.html
pub fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
    if let Ok(path) = path.as_ref().strip_prefix("~") {
        let mut expanded_path = PathBuf::new();
        expanded_path.push(get_home_dir().unwrap_or_else(|| "~".into()));
        expanded_path.push(path);
        expanded_path
    } else {
        path.as_ref().into()
    }
}

pub fn url_with_query(mut url: Url, query: &[(&str, Cow<str>)]) -> Url {
    if !query.is_empty() {
        // If we run this even without adding pairs it adds a `?`, hence
        // the .is_empty() check
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(name, value);
        }
    }
    url
}

// https://stackoverflow.com/a/45145246/5915221
#[macro_export]
macro_rules! vec_of_strings {
    ($($str:expr),*) => ({
        vec![$(String::from($str),)*] as Vec<String>
    });
}

/// When downloading a large file from a local nginx, it seems that 128KiB
/// is a bit faster than 64KiB but bumping it up to 256KiB doesn't help any
/// more.
/// When increasing the buffer size all the way to 1MiB I observe 408KiB as
/// the largest read size. But this doesn't translate to a shorter runtime.
pub const BUFFER_SIZE: usize = 128 * 1024;

/// io::copy, but with a larger buffer size.
///
/// io::copy's buffer is just 8 KiB. This noticeably slows down fast
/// large downloads, especially with a progress bar.
///
/// If `flush` is true, the writer will be flushed after each write. This is
/// appropriate for streaming output, where you don't want a delay between data
/// arriving and being shown.
pub fn copy_largebuf(
    reader: &mut impl io::Read,
    writer: &mut impl Write,
    flush: bool,
) -> io::Result<()> {
    let mut buf = vec![0; BUFFER_SIZE];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(len) => {
                writer.write_all(&buf[..len])?;
                if flush {
                    writer.flush()?;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

pub(crate) trait HeaderValueExt {
    fn to_utf8_str(&self) -> Result<&str, Utf8Error>;

    fn to_ascii_or_latin1(&self) -> Result<&str, BadHeaderValue<'_>>;
}

impl HeaderValueExt for HeaderValue {
    fn to_utf8_str(&self) -> Result<&str, Utf8Error> {
        std::str::from_utf8(self.as_bytes())
    }

    /// If the value is pure ASCII, return Ok(). If not, return Err() with methods for
    /// further handling.
    ///
    /// The Ok() version cannot contain control characters (not even ASCII ones).
    fn to_ascii_or_latin1(&self) -> Result<&str, BadHeaderValue<'_>> {
        self.to_str().map_err(|_| BadHeaderValue { value: self })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BadHeaderValue<'a> {
    value: &'a HeaderValue,
}

impl<'a> BadHeaderValue<'a> {
    /// Return the header value's latin1 decoding, AKA isomorphic decode,
    /// AKA ISO-8859-1 decode. This is how browsers tend to handle it.
    ///
    /// Not to be confused with ISO 8859-1 (which leaves 0x8X and 0x9X unmapped)
    /// or with Windows-1252 (which is how HTTP bodies are decoded if they
    /// declare `Content-Encoding: iso-8859-1`).
    ///
    /// Is likely to contain control characters. Consider replacing these.
    pub(crate) fn latin1(self) -> String {
        // https://infra.spec.whatwg.org/#isomorphic-decode
        self.value.as_bytes().iter().map(|&b| b as char).collect()
    }

    /// Return the header value's UTF-8 decoding. This is most likely what the
    /// user expects, but when browsers prefer another encoding we should give
    /// that one precedence.
    pub(crate) fn utf8(self) -> Option<&'a str> {
        self.value.to_utf8_str().ok()
    }
}

pub(crate) fn reason_phrase(response: &Response) -> Cow<'_, str> {
    if let Some(reason) = response.extensions().get::<hyper::ext::ReasonPhrase>() {
        // The server sent a non-standard reason phrase.
        // Seems like some browsers interpret this as latin1 and others as UTF-8?
        // Rare case and clients aren't supposed to pay attention to the reason
        // phrase so let's just do UTF-8 for convenience.
        // We could send the bytes straight to stdout/stderr in case they're some
        // other encoding but that's probably not worth the effort.
        String::from_utf8_lossy(reason.as_bytes())
    } else if let Some(reason) = response.status().canonical_reason() {
        // On HTTP/2+ no reason phrase is sent so we're just explaining the code
        // to the user.
        // On HTTP/1.1 and below this matches the reason the server actually sent
        // or else hyper would have added a ReasonPhrase.
        Cow::Borrowed(reason)
    } else {
        // Only reachable in case of an unknown status code over HTTP/2+.
        // curl prints nothing in this case.
        Cow::Borrowed("<unknown status code>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latin1() {
        let good = HeaderValue::from_static("Rhodes");
        let good = good.to_ascii_or_latin1();

        assert_eq!(good, Ok("Rhodes"));

        let bad = HeaderValue::from_bytes("Ῥόδος".as_bytes()).unwrap();
        let bad = bad.to_ascii_or_latin1().unwrap_err();

        assert_eq!(bad.latin1(), "á¿¬Ï\u{8c}Î´Î¿Ï\u{82}");
        assert_eq!(bad.utf8(), Some("Ῥόδος"));

        let worse = HeaderValue::from_bytes(b"R\xF3dos").unwrap();
        let worse = worse.to_ascii_or_latin1().unwrap_err();

        assert_eq!(worse.latin1(), "Ródos");
        assert_eq!(worse.utf8(), None);
    }
}

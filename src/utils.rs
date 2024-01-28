use std::borrow::Cow;
use std::env::var_os;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use reqwest::blocking::Request;
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
    // use an environment variable
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
        Some(dir.into())
    } else if cfg!(target_os = "macos") {
        dirs::home_dir().map(|dir| dir.join(".config").join("xh"))
    } else {
        dirs::config_dir().map(|dir| dir.join("xh"))
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

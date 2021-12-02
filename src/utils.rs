use std::env::var_os;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use reqwest::blocking::Request;
use url::{Host, Url};

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

// https://stackoverflow.com/a/45145246/5915221
#[macro_export]
macro_rules! vec_of_strings {
    ($($str:expr),*) => ({
        vec![$(String::from($str),)*] as Vec<String>
    });
}

#[macro_export]
macro_rules! regex {
    ($name:ident = $($re:expr)+) => {
        static $name: once_cell::sync::Lazy<regex::Regex> =
            once_cell::sync::Lazy::new(|| regex::Regex::new(concat!($($re,)+)).unwrap());
    };
    ($($re:expr)+) => {{
        static RE: once_cell::sync::Lazy<regex::Regex> =
            once_cell::sync::Lazy::new(|| regex::Regex::new(concat!($($re,)+)).unwrap());
        &RE
    }};
}

pub const BUFFER_SIZE: usize = 64 * 1024;

/// io::copy, but with a larger buffer size.
///
/// io::copy's buffer is just 8 KiB. This noticeably slows down fast
/// large downloads, especially with a progress bar.
///
/// This one's size of 64 KiB was chosen because that makes it competitive
/// with the old implementation, which repeatedly called .chunk().await.
///
/// Tests were done by running `ht -o /dev/null [-d]` on a two-gigabyte file
/// served locally by `python3 -m http.server`. Results may vary.
pub fn copy_largebuf(reader: &mut impl io::Read, writer: &mut impl Write) -> io::Result<()> {
    let mut buf = vec![0; BUFFER_SIZE];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(len) => writer.write_all(&buf[..len])?,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

pub(crate) fn url_requires_native_tls(url: &Url) -> bool {
    url.scheme() == "https" && matches!(url.host(), Some(Host::Ipv4(..)) | Some(Host::Ipv6(..)))
}

use std::env::var_os;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use brotli::Decompressor as BrotliDecoder;
use flate2::read::{GzDecoder, ZlibDecoder};
use reqwest::blocking::Request;
use reqwest::header::{HeaderMap, CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};
use url::{Host, Url};

pub fn clone_request(request: &mut Request) -> Result<Request> {
    if let Some(b) = request.body_mut().as_mut() {
        b.buffer()?;
    }
    // This doesn't copy the contents of the buffer, cloning requests is cheap
    // https://docs.rs/bytes/1.0.1/bytes/struct.Bytes.html
    Ok(request.try_clone().unwrap()) // guaranteed to not fail if body is already buffered
}

#[derive(Debug)]
pub enum CompressionType {
    Gzip,
    Deflate,
    Brotli,
}

impl FromStr for CompressionType {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> anyhow::Result<CompressionType> {
        match value {
            "gzip" => Ok(CompressionType::Gzip),
            "deflate" => Ok(CompressionType::Deflate),
            "br" => Ok(CompressionType::Brotli),
            _ => Err(anyhow::anyhow!("unknown compression type")),
        }
    }
}

// See https://github.com/seanmonstar/reqwest/blob/9bd4e90ec3401c2c5bc435c58954f3d52ab53e99/src/async_impl/decoder.rs#L150
pub fn get_compression_type(headers: &HeaderMap) -> Option<CompressionType> {
    let mut compression_type = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .find_map(|value| value.to_str().ok().and_then(|value| value.parse().ok()));

    if compression_type.is_none() {
        compression_type = headers
            .get_all(TRANSFER_ENCODING)
            .iter()
            .find_map(|value| value.to_str().ok().and_then(|value| value.parse().ok()));
    }

    if compression_type.is_some() {
        if let Some(content_length) = headers.get(CONTENT_LENGTH) {
            if content_length == "0" {
                return None;
            }
        }
    }

    compression_type
}

enum Decoder<R>
where
    R: Read,
{
    PlainText(R),
    Gzip(GzDecoder<R>),
    Deflate(ZlibDecoder<R>),
    Brotli(BrotliDecoder<R>),
}

impl<R> Read for Decoder<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Decoder::PlainText(decoder) => decoder.read(buf),
            Decoder::Gzip(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(e.kind(), format!("error decoding response body: {}", e))
            }),
            Decoder::Deflate(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(e.kind(), format!("error decoding response body: {}", e))
            }),
            Decoder::Brotli(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(e.kind(), format!("error decoding response body: {}", e))
            }),
        }
    }
}

pub fn decompress(
    reader: &mut impl Read,
    compression_type: Option<CompressionType>,
) -> impl Read + '_ {
    match compression_type {
        Some(CompressionType::Gzip) => Decoder::Gzip(GzDecoder::new(reader)),
        Some(CompressionType::Deflate) => Decoder::Deflate(ZlibDecoder::new(reader)),
        Some(CompressionType::Brotli) => Decoder::Brotli(BrotliDecoder::new(reader, 4096)),
        None => Decoder::PlainText(reader),
    }
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

pub fn url_requires_native_tls(url: &Url) -> bool {
    url.scheme() == "https" && matches!(url.host(), Some(Host::Ipv4(..)) | Some(Host::Ipv6(..)))
}

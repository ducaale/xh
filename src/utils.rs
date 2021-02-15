use std::{
    env::var_os,
    fs::File,
    io::{self, stdin, Read, Write},
    path::Path,
};

use anyhow::Result;
use atty::Stream;
use reqwest::{
    blocking::multipart,
    header::{HeaderMap, CONTENT_TYPE},
};

use crate::Body;

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

pub enum ContentType {
    Json,
    Html,
    Xml,
    UrlencodedForm,
    Multipart,
}

pub fn get_content_type(headers: &HeaderMap) -> Option<ContentType> {
    headers
        .get(CONTENT_TYPE)?
        .to_str()
        .ok()
        .and_then(|content_type| {
            if content_type.contains("json") {
                Some(ContentType::Json)
            } else if content_type.contains("html") {
                Some(ContentType::Html)
            } else if content_type.contains("xml") {
                Some(ContentType::Xml)
            } else if content_type.contains("multipart") {
                Some(ContentType::Multipart)
            } else if content_type.contains("x-www-form-urlencoded") {
                Some(ContentType::UrlencodedForm)
            } else {
                None
            }
        })
}

// https://github.com/seanmonstar/reqwest/issues/646#issuecomment-616985015
pub fn file_to_part(path: impl AsRef<Path>) -> io::Result<multipart::Part> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .map(|file_name| file_name.to_string_lossy().to_string());
    let file = File::open(path)?;
    let file_length = file.metadata()?.len();
    let mut part = multipart::Part::reader_with_length(file, file_length);
    if let Some(file_name) = file_name {
        part = part.file_name(file_name);
    }
    Ok(part)
}

pub fn body_from_stdin(ignore_stdin: bool) -> Result<Option<Body>> {
    if atty::is(Stream::Stdin) || ignore_stdin || test_pretend_term() {
        Ok(None)
    } else {
        let mut buffer = String::new();
        stdin().read_to_string(&mut buffer)?;
        Ok(Some(Body::Raw(buffer)))
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
        lazy_static::lazy_static! {
            static ref $name: regex::Regex = regex::Regex::new(concat!($($re,)+)).unwrap();
        }
    };
    ($($re:expr)+) => {{
        lazy_static::lazy_static! {
            static ref RE: regex::Regex = regex::Regex::new(concat!($($re,)+)).unwrap();
        }
        &RE
    }};
}

const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;

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
    let mut buf = vec![0; DOWNLOAD_BUFFER_SIZE];
    let mut buf = buf.as_mut_slice();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(len) => writer.write_all(&buf[..len])?,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

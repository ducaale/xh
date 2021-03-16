use std::{
    env::var_os,
    io::{self, Write},
};

use reqwest::header::{HeaderMap, CONTENT_TYPE};

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

pub enum ContentType {
    Json,
    Html,
    Xml,
    JavaScript,
    Css,
    Text,
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
            } else if content_type.contains("javascript") {
                Some(ContentType::JavaScript)
            } else if content_type.contains("css") {
                Some(ContentType::Css)
            } else if content_type.contains("text") {
                // We later check if this one's JSON
                // HTTPie checks for "json", "javascript" and "text" in one place:
                // https://github.com/httpie/httpie/blob/a32ad344dd/httpie/output/formatters/json.py#L14
                // We have it more spread out but it behaves more or less the same
                Some(ContentType::Text)
            } else {
                None
            }
        })
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

pub fn valid_json(text: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
}

use std::fs::File;
use std::io::{self, stdin, Read};
use std::path::Path;

use anyhow::Result;
use atty::Stream;
use reqwest::{
    blocking::multipart,
    header::{HeaderMap, CONTENT_TYPE},
};

use crate::Body;

pub fn test_mode() -> bool {
    cfg!(test) || std::env::var_os("XH_TEST_MODE").is_some()
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
    if atty::is(Stream::Stdin) || ignore_stdin {
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

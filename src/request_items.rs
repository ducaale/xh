use std::{
    borrow::Cow,
    collections::HashSet,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{blocking::multipart, Method};

use crate::cli::BodyType;
use crate::nested_json;
use crate::utils::{expand_tilde, unescape};

pub const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const JSON_ACCEPT: &str = "application/json, */*;q=0.5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestItem {
    HttpHeader(String, String),
    HttpHeaderFromFile(String, String),
    HttpHeaderToUnset(String),
    UrlParam(String, String),
    UrlParamFromFile(String, String),
    DataField {
        key: String,
        raw_key: String,
        value: String,
    },
    DataFieldFromFile {
        key: String,
        raw_key: String,
        value: String,
    },
    JsonField(String, serde_json::Value),
    JsonFieldFromFile(String, String),
    FormFile {
        key: String,
        file_name: String,
        file_type: Option<String>,
        file_name_header: Option<String>,
    },
}

impl FromStr for RequestItem {
    type Err = clap::Error;
    fn from_str(request_item: &str) -> clap::Result<RequestItem> {
        const SPECIAL_CHARS: &str = "=@:;\\";
        const SEPS: &[&str] = &["==@", "=@", ":=@", ":@", "==", ":=", "=", "@", ":"];

        fn split(request_item: &str) -> Option<(&str, &'static str, &str)> {
            let mut char_inds = request_item.char_indices();
            while let Some((ind, ch)) = char_inds.next() {
                if ch == '\\' {
                    // If the next character is special it's escaped and can't be
                    // the start of the separator
                    // And if it's normal it can't be the start either
                    // Just skip it without looking
                    char_inds.next();
                    continue;
                }
                for sep in SEPS {
                    if let Some(value) = request_item[ind..].strip_prefix(sep) {
                        let key = &request_item[..ind];
                        return Some((key, sep, value));
                    }
                }
            }
            None
        }

        if let Some((raw_key, sep, value)) = split(request_item) {
            let raw_key = raw_key.to_string();
            let key = unescape(&raw_key, SPECIAL_CHARS);
            let value = unescape(value, SPECIAL_CHARS);
            match sep {
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField {
                    key,
                    raw_key,
                    value,
                }),
                ":=" => Ok(RequestItem::JsonField(
                    raw_key,
                    serde_json::from_str(&value).map_err(|err| {
                        clap::Error::raw(
                            clap::ErrorKind::InvalidValue,
                            format!(
                                "Invalid value for '[REQUEST_ITEM]...': {:?} {}",
                                request_item, err
                            ),
                        )
                    })?,
                )),
                "@" => {
                    let PartWithParams {
                        value,
                        file_type,
                        file_name_header,
                    } = parse_part_params(&value);
                    Ok(RequestItem::FormFile {
                        key,
                        file_name: value,
                        file_type,
                        file_name_header,
                    })
                }
                ":" if value.is_empty() => Ok(RequestItem::HttpHeaderToUnset(key)),
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==@" => Ok(RequestItem::UrlParamFromFile(key, value)),
                "=@" => Ok(RequestItem::DataFieldFromFile {
                    key,
                    raw_key,
                    value,
                }),
                ":=@" => Ok(RequestItem::JsonFieldFromFile(raw_key, value)),
                ":@" => Ok(RequestItem::HttpHeaderFromFile(key, value)),
                _ => unreachable!(),
            }
        } else if let Some(header) = request_item.strip_suffix(';') {
            // Technically this is too permissive because the ; might be escaped
            Ok(RequestItem::HttpHeader(header.to_owned(), "".to_owned()))
        } else {
            // TODO: We can also end up here if the method couldn't be parsed
            // and was interpreted as a URL, making the actual URL a request
            // item
            Err(clap::Error::raw(
                clap::ErrorKind::InvalidValue,
                format!("Invalid value for '[REQUEST_ITEM]...': {:?}", request_item),
            ))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PartWithParams {
    value: String,
    file_type: Option<String>,
    file_name_header: Option<String>,
}

/// HTTPie's syntax for this is imitating curl's.
///
/// curl's syntax is pretty hairy. At the most basic level it's just key-value
/// pairs separated by semicolons, but:
/// - Values may be quoted. This stops spaces from being stripped and allows
///   you to put semicolons in values. (Between quotes, quotes and backslashes
///   have to be backslash-escaped.)
/// - If a key is not recognized then it's skipped with a warning.
///   - Unless it comes right after a mimetype, in which case it's seen as part
///     of the last value, because mimetypes can use the exact same syntax
///     (e.g. `text/html; charset=UTF-8`).
///     `;type=text/plain;filename=foobar` will send Content-Type `text/plain`
///     and filename `foobar`, but `;type=text/plain;foo=bar` will send
///     Content-Type `text/plain;foo=bar`.
///
/// We'll cut some corners and just split on ";type=" and ";filename=". That should
/// be good enough for most purposes. (HTTPie only splits on ";type=".)
fn parse_part_params(mut text: &str) -> PartWithParams {
    const TYPE_SEP: &str = ";type=";
    const FNAME_SEP: &str = ";filename=";

    let mut file_type = None;
    let mut file_name_header = None;

    // Look for parameters starting from the right.
    // Only look for a parameter as long as it hasn't been found yet.
    // (There may be a cleaner way, this is the best I could come up with.)
    let mut delims = vec![TYPE_SEP, FNAME_SEP];
    while let Some((pre, delim, post)) = rsplit_once_any(text, &delims) {
        match delim {
            TYPE_SEP => file_type = Some(post.to_owned()),
            FNAME_SEP => file_name_header = Some(post.to_owned()),
            _ => unreachable!(),
        }
        delims.retain(|&x| x != delim);
        text = pre;
    }

    PartWithParams {
        value: text.to_owned(),
        file_type,
        file_name_header,
    }
}

/// Find the rightmost match of any of the delimiters and do a split.
fn rsplit_once_any<'a, 'b>(
    text: &'a str,
    delimiters: &'b [&'static str],
) -> Option<(&'a str, &'static str, &'a str)> {
    let mut res = None;
    let mut best = 0;
    for &delim in delimiters {
        if let Some(pos) = text.rfind(delim) {
            if pos >= best {
                best = pos;
                res = Some((&text[..pos], delim, &text[pos + delim.len()..]));
            }
        }
    }
    res
}

#[derive(Default, Debug)]
pub struct RequestItems {
    pub items: Vec<RequestItem>,
    pub body_type: BodyType,
}

pub enum Body {
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Multipart(multipart::Form),
    Raw(Vec<u8>),
    File {
        file_name: PathBuf,
        file_type: Option<HeaderValue>,
        file_name_header: Option<String>,
    },
}

impl Body {
    pub fn is_empty(&self) -> bool {
        match self {
            Body::Json(value) => value.is_null(),
            Body::Form(items) => items.is_empty(),
            // A multipart form without items isn't empty, and we can't read
            // a body from stdin because it has to match the header, so we
            // should never consider this "empty"
            // This is a slight divergence from HTTPie, which will simply
            // discard stdin if it receives --multipart without request items,
            // but that behavior is useless so there's no need to match it
            Body::Multipart(..) => false,
            Body::File { .. } => false,
            Body::Raw(..) => false,
        }
    }

    pub fn pick_method(&self) -> Method {
        if self.is_empty() {
            Method::GET
        } else {
            Method::POST
        }
    }
}

impl RequestItems {
    pub fn has_form_files(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, RequestItem::FormFile { .. }))
    }

    pub fn headers(&self) -> Result<(HeaderMap<HeaderValue>, HashSet<HeaderName>)> {
        let mut headers = HeaderMap::new();
        #[allow(clippy::mutable_key_type)]
        let mut headers_to_unset = HashSet::new();
        for item in &self.items {
            match item {
                RequestItem::HttpHeader(key, value) => {
                    let key = HeaderName::from_bytes(key.as_bytes())?;
                    let value = HeaderValue::from_str(value)?;
                    headers_to_unset.remove(&key);
                    headers.append(key, value);
                }
                RequestItem::HttpHeaderFromFile(key, value) => {
                    let key = HeaderName::from_bytes(key.as_bytes())?;
                    let value = fs::read_to_string(expand_tilde(value))?;
                    let value = HeaderValue::from_str(value.trim())?;
                    headers_to_unset.remove(&key);
                    headers.append(key, value);
                }
                RequestItem::HttpHeaderToUnset(key) => {
                    let key = HeaderName::from_bytes(key.as_bytes())?;
                    headers.remove(&key);
                    headers_to_unset.insert(key);
                }
                RequestItem::UrlParam(..) => {}
                RequestItem::UrlParamFromFile(..) => {}
                RequestItem::DataField { .. } => {}
                RequestItem::DataFieldFromFile { .. } => {}
                RequestItem::JsonField(..) => {}
                RequestItem::JsonFieldFromFile(..) => {}
                RequestItem::FormFile { .. } => {}
            }
        }
        Ok((headers, headers_to_unset))
    }

    pub fn query(&self) -> Result<Vec<(&str, Cow<str>)>> {
        let mut query: Vec<(&str, Cow<str>)> = vec![];
        for item in &self.items {
            if let RequestItem::UrlParam(key, value) = item {
                query.push((key, Cow::Borrowed(value)));
            } else if let RequestItem::UrlParamFromFile(key, value) = item {
                let value = fs::read_to_string(expand_tilde(value))?;
                query.push((key, Cow::Owned(value)));
            }
        }
        Ok(query)
    }

    fn body_as_json(self) -> Result<Body> {
        use serde_json::Value;
        let mut body = None;
        for item in self.items {
            let (raw_key, value) = match item {
                RequestItem::JsonField(raw_key, value) => (raw_key, value),
                RequestItem::JsonFieldFromFile(raw_key, value) => {
                    let value = serde_json::from_str(&fs::read_to_string(expand_tilde(value))?)?;
                    (raw_key, value)
                }
                RequestItem::DataField { raw_key, value, .. } => (raw_key, Value::String(value)),
                RequestItem::DataFieldFromFile { raw_key, value, .. } => {
                    let value = fs::read_to_string(expand_tilde(value))?;
                    (raw_key, Value::String(value))
                }
                RequestItem::FormFile { .. } => unreachable!(),
                RequestItem::HttpHeader(..)
                | RequestItem::HttpHeaderFromFile(..)
                | RequestItem::HttpHeaderToUnset(..)
                | RequestItem::UrlParam(..)
                | RequestItem::UrlParamFromFile(..) => continue,
            };
            let json_path = nested_json::parse_path(&raw_key)?;
            body = nested_json::insert(body, &json_path, value)
                .map_err(|e| e.with_json_path(raw_key))?
                .into();
        }
        Ok(Body::Json(body.unwrap_or(Value::Null)))
    }

    fn body_as_form(self) -> Result<Body> {
        let mut text_fields = Vec::<(String, String)>::new();
        for item in self.items {
            match item {
                RequestItem::JsonField(..) | RequestItem::JsonFieldFromFile(..) => {
                    return Err(anyhow!("JSON values are not supported in Form fields"));
                }
                RequestItem::DataField { key, value, .. } => text_fields.push((key, value)),
                RequestItem::DataFieldFromFile { key, value, .. } => {
                    let path = expand_tilde(value);
                    text_fields.push((key, fs::read_to_string(path)?));
                }
                RequestItem::FormFile { .. } => unreachable!(),
                RequestItem::HttpHeader(..) => {}
                RequestItem::HttpHeaderFromFile(..) => {}
                RequestItem::HttpHeaderToUnset(..) => {}
                RequestItem::UrlParam(..) => {}
                RequestItem::UrlParamFromFile(..) => {}
            }
        }
        Ok(Body::Form(text_fields))
    }

    fn body_as_multipart(self) -> Result<Body> {
        let mut form = multipart::Form::new();
        for item in self.items {
            match item {
                RequestItem::JsonField(..) | RequestItem::JsonFieldFromFile(..) => {
                    return Err(anyhow!("JSON values are not supported in multipart fields"));
                }
                RequestItem::DataField { key, value, .. } => {
                    form = form.text(key, value);
                }
                RequestItem::DataFieldFromFile { key, value, .. } => {
                    let path = expand_tilde(value);
                    form = form.text(key, fs::read_to_string(path)?);
                }
                RequestItem::FormFile {
                    key,
                    file_name,
                    file_type,
                    file_name_header,
                } => {
                    let mut part = file_to_part(expand_tilde(file_name))?;
                    if let Some(file_type) = file_type {
                        part = part.mime_str(&file_type)?;
                    }
                    if let Some(file_name_header) = file_name_header {
                        part = part.file_name(file_name_header);
                    }
                    form = form.part(key, part);
                }
                RequestItem::HttpHeader(..) => {}
                RequestItem::HttpHeaderFromFile(..) => {}
                RequestItem::HttpHeaderToUnset(..) => {}
                RequestItem::UrlParam(..) => {}
                RequestItem::UrlParamFromFile(..) => {}
            }
        }
        Ok(Body::Multipart(form))
    }

    fn body_from_file(self) -> Result<Body> {
        let mut body = None;
        if self
            .items
            .iter()
            .any(|item| matches!(item, RequestItem::FormFile {key, ..} if !key.is_empty()))
        {
            return Err(anyhow!(
                "Can't use file fields in JSON mode (perhaps you meant --form?)"
            ));
        }
        for item in self.items {
            match item {
                RequestItem::DataField { .. }
                | RequestItem::JsonField(..)
                | RequestItem::DataFieldFromFile { .. }
                | RequestItem::JsonFieldFromFile(..) => {
                    return Err(anyhow!(
                        "Request body (from a file) and request data (key=value) cannot be mixed."
                    ));
                }
                RequestItem::FormFile {
                    key,
                    file_name,
                    file_type,
                    file_name_header,
                } => {
                    assert!(key.is_empty());
                    if body.is_some() {
                        return Err(anyhow!("Can't read request from multiple files"));
                    }
                    body = Some(Body::File {
                        file_type: file_type
                            .as_deref()
                            .or_else(|| mime_guess::from_path(&file_name).first_raw())
                            .map(HeaderValue::from_str)
                            .transpose()?,
                        file_name: expand_tilde(file_name),
                        file_name_header,
                    });
                }
                RequestItem::HttpHeader(..)
                | RequestItem::HttpHeaderFromFile(..)
                | RequestItem::HttpHeaderToUnset(..)
                | RequestItem::UrlParam(..)
                | RequestItem::UrlParamFromFile(..) => {}
            }
        }
        let body = body.expect("Should have had at least one file field");
        Ok(body)
    }

    pub fn body(self) -> Result<Body> {
        match self.body_type {
            BodyType::Multipart => self.body_as_multipart(),
            BodyType::Form if self.has_form_files() => self.body_as_multipart(),
            BodyType::Form => self.body_as_form(),
            BodyType::Json if self.has_form_files() => self.body_from_file(),
            BodyType::Json => self.body_as_json(),
        }
    }

    /// Determine whether a multipart request should be used.
    ///
    /// This duplicates logic in `body()` for the benefit of `to_curl`.
    pub fn is_multipart(&self) -> bool {
        match self.body_type {
            BodyType::Multipart => true,
            BodyType::Form => self.has_form_files(),
            BodyType::Json => false,
        }
    }

    /// Guess which HTTP method would be appropriate for the return value of `body`.
    ///
    /// It's better to use `Body::pick_method`, if possible. This method is
    /// for the benefit of `to_curl`, which sometimes has to process the
    /// request items itself.
    pub fn pick_method(&self) -> Method {
        if self.body_type == BodyType::Multipart {
            return Method::POST;
        }
        for item in &self.items {
            match item {
                RequestItem::HttpHeader(..)
                | RequestItem::HttpHeaderFromFile(..)
                | RequestItem::HttpHeaderToUnset(..)
                | RequestItem::UrlParam(..)
                | RequestItem::UrlParamFromFile(..) => continue,
                RequestItem::DataField { .. }
                | RequestItem::DataFieldFromFile { .. }
                | RequestItem::JsonField(..)
                | RequestItem::JsonFieldFromFile(..)
                | RequestItem::FormFile { .. } => return Method::POST,
            }
        }
        Method::GET
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn request_item_parsing() {
        use serde_json::json;

        use RequestItem::*;

        fn parse(text: &str) -> RequestItem {
            text.parse().unwrap()
        }

        // Data field
        assert_eq!(
            parse("foo=bar"),
            DataField {
                key: "foo".into(),
                raw_key: "foo".into(),
                value: "bar".into()
            }
        );
        // Data field from file
        assert_eq!(
            parse("foo=@data.json"),
            DataFieldFromFile {
                key: "foo".into(),
                raw_key: "foo".into(),
                value: "data.json".into()
            }
        );
        // URL param
        assert_eq!(parse("foo==bar"), UrlParam("foo".into(), "bar".into()));
        // URL param from file
        assert_eq!(
            parse("foo==@data.txt"),
            UrlParamFromFile("foo".into(), "data.txt".into())
        );
        // Escaped right before separator
        assert_eq!(
            parse(r"foo\==bar"),
            DataField {
                key: "foo=".into(),
                raw_key: r"foo\=".into(),
                value: "bar".into()
            }
        );
        // Header
        assert_eq!(parse("foo:bar"), HttpHeader("foo".into(), "bar".into()));
        // Header from file
        assert_eq!(
            parse("foo:@data.txt"),
            HttpHeaderFromFile("foo".into(), "data.txt".into())
        );
        // JSON field
        assert_eq!(parse("foo:=[1,2]"), JsonField("foo".into(), json!([1, 2])));
        // JSON field from file
        assert_eq!(
            parse("foo:=@data.json"),
            JsonFieldFromFile("foo".into(), "data.json".into())
        );
        // Bad JSON field
        "foo:=bar".parse::<RequestItem>().unwrap_err();
        // Can't escape normal chars
        assert_eq!(
            parse(r"f\o\o=\ba\r"),
            DataField {
                key: r"f\o\o".into(),
                raw_key: r"f\o\o".into(),
                value: r"\ba\r".into()
            },
        );
        // Can escape special chars
        assert_eq!(
            parse(r"f\=\:\@\;oo=b\:\:\:ar"),
            DataField {
                key: "f=:@;oo".into(),
                raw_key: r"f\=\:\@\;oo".into(),
                value: "b:::ar".into()
            },
        );
        // Unset header
        assert_eq!(parse("foobar:"), HttpHeaderToUnset("foobar".into()));
        // Empty header
        assert_eq!(parse("foobar;"), HttpHeader("foobar".into(), "".into()));
        // Untyped file
        assert_eq!(
            parse("foo@bar"),
            FormFile {
                key: "foo".into(),
                file_name: "bar".into(),
                file_type: None,
                file_name_header: None,
            }
        );
        // Typed file
        assert_eq!(
            parse("foo@bar;type=qux"),
            FormFile {
                key: "foo".into(),
                file_name: "bar".into(),
                file_type: Some("qux".into()),
                file_name_header: None,
            },
        );
        // Multi-typed file
        assert_eq!(
            parse("foo@bar;type=qux;type=qux"),
            FormFile {
                key: "foo".into(),
                file_name: "bar;type=qux".into(),
                file_type: Some("qux".into()),
                file_name_header: None,
            },
        );
        // Empty filename
        // (rejecting this would be fine too, the main point is to see if it panics)
        assert_eq!(
            parse("foo@"),
            FormFile {
                key: "foo".into(),
                file_name: "".into(),
                file_type: None,
                file_name_header: None,
            }
        );
        // No separator
        "foobar".parse::<RequestItem>().unwrap_err();
        "".parse::<RequestItem>().unwrap_err();
        // Trailing backslash
        assert_eq!(
            parse(r"foo=bar\"),
            DataField {
                key: "foo".into(),
                raw_key: "foo".into(),
                value: r"bar\".into()
            }
        );
        // Escaped backslash
        assert_eq!(
            parse(r"foo\\=bar"),
            DataField {
                key: r"foo\".into(),
                raw_key: r"foo\\".into(),
                value: "bar".into()
            }
        );
        // Unicode
        assert_eq!(
            parse("\u{00B5}=\u{00B5}"),
            DataField {
                key: "\u{00B5}".into(),
                raw_key: "\u{00B5}".into(),
                value: "\u{00B5}".into()
            },
        );
        // Empty
        assert_eq!(
            parse("="),
            DataField {
                key: "".into(),
                raw_key: "".into(),
                value: "".into()
            }
        );
    }

    #[test]
    fn param_parsing() {
        assert_eq!(
            parse_part_params("foo;type=bar;filename=baz"),
            PartWithParams {
                value: "foo".into(),
                file_type: Some("bar".into()),
                file_name_header: Some("baz".into()),
            }
        );
        assert_eq!(
            parse_part_params(";type=foo"),
            PartWithParams {
                value: "".into(),
                file_type: Some("foo".into()),
                file_name_header: None,
            }
        );
        assert_eq!(
            parse_part_params("foo;type=bar;type=baz;filename=qux"),
            PartWithParams {
                value: "foo;type=bar".into(),
                file_type: Some("baz".into()),
                file_name_header: Some("qux".into()),
            }
        );
        assert_eq!(
            parse_part_params("foo;type=bar;filename=qux;type=baz"),
            PartWithParams {
                value: "foo;type=bar".into(),
                file_type: Some("baz".into()),
                file_name_header: Some("qux".into()),
            }
        );
        assert_eq!(
            parse_part_params("foo;x=y"),
            PartWithParams {
                value: "foo;x=y".into(),
                file_type: None,
                file_name_header: None,
            }
        );
        assert_eq!(
            parse_part_params(""),
            PartWithParams {
                value: "".into(),
                file_type: None,
                file_name_header: None,
            }
        );
    }
}

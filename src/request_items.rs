use std::error::Error;
use std::str::FromStr;

use regex::Regex;
use reqwest::blocking::multipart;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, HOST,
};
use reqwest::Url;

#[derive(Debug, Clone)]
enum RequestItem {
    HttpHeader(String, String),
    UrlParam(String, String),
    DataField(String, String),
    JSONField(String, serde_json::Value),
    FormFile(String, String),
}

impl FromStr for RequestItem {
    type Err = Box<dyn Error>;
    fn from_str(request_item: &str) -> Result<RequestItem, Box<dyn Error>> {
        let re = Regex::new(r"^(.+?)(==|:=|=|@|:)(.+)$").unwrap();
        if let Some(caps) = re.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[3].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField(key, value)),
                ":=" => Ok(RequestItem::JSONField(
                    key,
                    serde_json::from_str(&value).unwrap(),
                )),
                "@" => Ok(RequestItem::FormFile(key, value)),
                _ => unreachable!(),
            }
        } else {
            // TODO: replace panic with error
            panic!(format!("{:?} is not a valid value", request_item))
        }
    }
}

pub enum Body {
    Json(serde_json::Map<String, serde_json::Value>),
    Form(Vec<(String, String)>),
    Multipart(multipart::Form),
}

pub struct RequestItems(Vec<RequestItem>);

impl From<Vec<String>> for RequestItems {
    fn from(request_items: Vec<String>) -> RequestItems {
        RequestItems(
            request_items.iter()
                .map(|r| RequestItem::from_str(r).unwrap())
                .collect()
        )
    }
}

impl RequestItems {
    pub fn new(request_items: Vec<String>) -> RequestItems {
        request_items.into()
    }

    pub fn headers(&self, url: &Url) -> HeaderMap<HeaderValue> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.insert(
            HOST,
            HeaderValue::from_str(&url.host().unwrap().to_string()).unwrap(),
        );
        for item in &self.0 {
            match item {
                RequestItem::HttpHeader(key, value) => {
                    let key = HeaderName::from_bytes(&key.as_bytes()).unwrap();
                    let value = HeaderValue::from_str(&value).unwrap();
                    headers.insert(key, value);
                }
                _ => {}
            }
        }
        headers
    }

    pub fn query(&self) -> Vec<(&String, &String)> {
        let mut query = vec![];
        for item in &self.0 {
            match item {
                RequestItem::UrlParam(key, value) => {
                    query.push((key, value));
                }
                _ => {}
            }
        }
        query
    }

    pub fn body(&self, as_form: bool) -> Option<Body> {
        if !as_form {
            let mut body = serde_json::Map::new();
            for item in &self.0 {
                match item.clone() {
                    RequestItem::JSONField(key, value) => {
                        body.insert(key, value);
                    }
                    RequestItem::DataField(key, value) => {
                        body.insert(key, serde_json::Value::String(value));
                    }
                    RequestItem::FormFile(_, _) => panic!("boom!"),
                    _ => {}
                }
            }
            if body.len() > 0 {
                Some(Body::Json(body))
            } else {
                None
            }
        } else {
            let mut text_fields = Vec::<(String, String)>::new();
            let mut files = Vec::<(String, String)>::new();
            for item in &self.0 {
                match item.clone() {
                    RequestItem::JSONField(_, _) => panic!("boom"),
                    RequestItem::DataField(key, value) => text_fields.push((key, value)),
                    RequestItem::FormFile(key, value) => files.push((key, value)),
                    _ => {}
                }
            }
            match (text_fields.len(), files.len()) {
                (0, 0) => None,
                (_, 0) => Some(Body::Form(text_fields)),
                (_, _) => {
                    let mut form = multipart::Form::new();
                    for (key, value) in text_fields {
                        form = form.text(key, value);
                    }
                    for (key, value) in files {
                        form = form.file(key, value).unwrap();
                    }
                    Some(Body::Multipart(form))
                }
            }
        }
    }
}

use reqwest::blocking::multipart;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::RequestItem;

pub struct RequestItems(Vec<RequestItem>);

pub enum Body {
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Multipart(multipart::Form),
    Raw(String),
}

impl RequestItems {
    pub fn new(request_items: Vec<RequestItem>) -> RequestItems {
        RequestItems(request_items)
    }

    fn form_file_count(&self) -> usize {
        let mut count = 0;
        for item in &self.0 {
            match item {
                RequestItem::FormFile(_, _) => count += 1,
                _ => {}
            }
        }
        count
    }

    pub fn headers(&self) -> (HeaderMap<HeaderValue>, Vec<HeaderName>) {
        let mut headers = HeaderMap::new();
        let mut headers_to_unset = vec![];
        for item in &self.0 {
            match item {
                RequestItem::HttpHeader(key, value) => {
                    let key = HeaderName::from_bytes(&key.as_bytes()).unwrap();
                    let value = HeaderValue::from_str(&value).unwrap();
                    headers.insert(key, value);
                }
                RequestItem::HttpHeaderToUnset(key) => {
                    let key = HeaderName::from_bytes(&key.as_bytes()).unwrap();
                    headers_to_unset.push(key);
                }
                _ => {}
            }
        }
        (headers, headers_to_unset)
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

    fn body_as_json(&self) -> Result<Option<Body>, &str> {
        let mut body = serde_json::Map::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(key, value) => {
                    body.insert(key, value);
                }
                RequestItem::DataField(key, value) => {
                    body.insert(key, serde_json::Value::String(value));
                }
                RequestItem::FormFile(_, _) => {
                    return Err(
                        "Sending Files is not supported when the request body is in JSON format",
                    );
                }
                _ => {}
            }
        }
        if body.len() > 0 {
            Ok(Some(Body::Json(body.into())))
        } else {
            Ok(None)
        }
    }

    fn body_as_form(&self) -> Result<Option<Body>, &str> {
        let mut text_fields = Vec::<(String, String)>::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(_, _) => {
                    return Err("JSON values are not supported in Form fields");
                }
                RequestItem::DataField(key, value) => text_fields.push((key, value)),
                _ => {}
            }
        }
        Ok(Some(Body::Form(text_fields)))
    }

    fn body_as_multipart(&self) -> Result<Option<Body>, &str> {
        let mut form = multipart::Form::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(_, _) => {
                    return Err("JSON values are not supported in multipart fields");
                }
                RequestItem::DataField(key, value) => {
                    form = form.text(key, value);
                }
                RequestItem::FormFile(key, value) => {
                    form = form.file(key, value).unwrap();
                }
                _ => {}
            }
        }
        Ok(Some(Body::Multipart(form)))
    }

    pub fn body(&self, form: bool, multipart: bool) -> Result<Option<Body>, &str> {
        match (form, multipart) {
            (_, true) => self.body_as_multipart(),
            (true, _) if self.form_file_count() > 0 => self.body_as_multipart(),
            (true, _) => self.body_as_form(),
            (_, _) => self.body_as_json(),
        }
    }
}

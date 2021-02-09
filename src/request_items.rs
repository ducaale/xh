use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

use crate::utils::file_to_part;
use crate::RequestItem;
use crate::Session;

pub struct RequestItems(Vec<RequestItem>);

pub enum Body {
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Multipart(multipart::Form),
    Raw(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub value: String,
}

impl RequestItems {
    pub fn new(request_items: Vec<RequestItem>) -> RequestItems {
        RequestItems(request_items)
    }

    fn form_file_count(&self) -> usize {
        self.0
            .iter()
            .filter(|item| matches!(item, RequestItem::FormFile(..)))
            .count()
    }

    pub fn headers(&self, session: Option<&Session>) -> Result<(HeaderMap<HeaderValue>, Vec<HeaderName>)> {
        let mut headers = HeaderMap::new();
        let mut headers_to_unset = vec![];
        for item in &self.0 {
            match item {
                RequestItem::HttpHeader(key, value) => {
                    let key = HeaderName::from_bytes(&key.as_bytes())?;
                    let value = HeaderValue::from_str(&value)?;
                    headers.insert(key, value);
                }
                RequestItem::HttpHeaderToUnset(key) => {
                    let key = HeaderName::from_bytes(&key.as_bytes())?;
                    headers_to_unset.push(key);
                }
                _ => {}
            }
        }
        // handle session additional headers
        match session {
            None => (),
            Some(s) => {
                for h in &*s.headers {
                    let key = HeaderName::from_bytes(&h.name.as_bytes()).unwrap();
                    headers
                        .entry(key)
                        .or_insert(HeaderValue::from_str(&h.value).unwrap());
                }
            }
        }
        Ok((headers, headers_to_unset))
    }

    pub fn export_headers(&self, session: Option<&Session>) -> Vec<Parameter> {
        let mut headers = vec![];
        let mut headernames_present = vec![];
        for item in &self.0 {
            match item {
                RequestItem::HttpHeader(key, value) => {
                    headers.push(Parameter {
                        name: String::from(key),
                        value: String::from(value),
                    });
                    headernames_present.push(String::from(key));
                }
                _ => {}
            }
        }
        // handle session additional headers
        match session {
            None => (),
            Some(s) => {
                for h in &s.headers {
                    if !headernames_present.contains(&h.name) {
                        headers.push(h.clone());
                    }
                }
            }
        }
        headers
    }

    pub fn query(&self) -> Vec<(&String, &String)> {
        let mut query = vec![];
        for item in &self.0 {
            if let RequestItem::UrlParam(key, value) = item {
                query.push((key, value));
            }
        }
        query
    }

    fn body_as_json(&self) -> Result<Option<Body>> {
        let mut body = serde_json::Map::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(key, value) => {
                    body.insert(key, value);
                }
                RequestItem::DataField(key, value) => {
                    body.insert(key, serde_json::Value::String(value));
                }
                RequestItem::FormFile(_, _, _) => {
                    return Err(anyhow!(
                        "Sending Files is not supported when the request body is in JSON format"
                    ));
                }
                _ => {}
            }
        }
        if !body.is_empty() {
            Ok(Some(Body::Json(body.into())))
        } else {
            Ok(None)
        }
    }

    fn body_as_form(&self) -> Result<Option<Body>> {
        let mut text_fields = Vec::<(String, String)>::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(_, _) => {
                    return Err(anyhow!("JSON values are not supported in Form fields"));
                }
                RequestItem::DataField(key, value) => text_fields.push((key, value)),
                _ => {}
            }
        }
        Ok(Some(Body::Form(text_fields)))
    }

    async fn body_as_multipart(&self) -> Result<Option<Body>> {
        let mut form = multipart::Form::new();
        for item in &self.0 {
            match item.clone() {
                RequestItem::JSONField(_, _) => {
                    return Err(anyhow!("JSON values are not supported in multipart fields"));
                }
                RequestItem::DataField(key, value) => {
                    form = form.text(key, value);
                }
                RequestItem::FormFile(key, value, file_type) => {
                    let mut part = file_to_part(&value).await?;
                    if let Some(file_type) = file_type {
                        part = part.mime_str(&file_type)?;
                    }
                    form = form.part(key, part);
                }
                _ => {}
            }
        }
        Ok(Some(Body::Multipart(form)))
    }

    pub async fn body(&self, form: bool, multipart: bool) -> Result<Option<Body>> {
        match (form, multipart) {
            (_, true) => self.body_as_multipart().await,
            (true, _) if self.form_file_count() > 0 => self.body_as_multipart().await,
            (true, _) => self.body_as_form(),
            (_, _) => self.body_as_json(),
        }
    }
}

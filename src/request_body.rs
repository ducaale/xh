use reqwest::blocking::multipart;

use crate::{Opt, RequestItem};

// TODO: replace this with RequestItem enum from cli.rs
// and add methods for getting headers, url_params, and
// an enum body that is either json, form or url_encoded
pub enum RequestBody {
    JSON(serde_json::Map<String, serde_json::Value>),
    Form {
        text: Vec<(String, String)>,
        files: Vec<(String, String)>,
    },
}

impl RequestBody {
    pub fn new(opt: &Opt) -> RequestBody {
        if opt.form {
            RequestBody::Form {
                text: vec![],
                files: vec![],
            }
        } else {
            RequestBody::JSON(serde_json::Map::new())
        }
    }

    pub fn insert(&mut self, request_item: RequestItem) {
        match request_item {
            RequestItem::DataField(key, value) => match self {
                RequestBody::Form { ref mut text, .. } => {
                    text.push((key, value));
                }
                RequestBody::JSON(ref mut body) => {
                    body.insert(key, serde_json::Value::String(value));
                }
            },
            RequestItem::JSONField(key, value) => match self {
                RequestBody::Form { .. } => panic!("json values cannot be used with forms"),
                RequestBody::JSON(ref mut body) => {
                    body.insert(key, value);
                }
            },
            RequestItem::FormFile(_, _) => todo!(),
            _ => unreachable!()
        }
    }

    pub fn json(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        if let RequestBody::JSON(body) = self {
            if body.len() == 0 {
                None
            } else {
                Some(body)
            }
        } else {
            None
        }
    }

    pub fn form(&self) -> Option<&Vec<(String, String)>> {
        if let RequestBody::Form { text, files } = self {
            if text.len() == 0 || files.len() > 0 {
                None
            } else {
                Some(text)
            }
        } else {
            None
        }
    }

    pub fn multipart(&self) -> Option<multipart::Form> {
        if let RequestBody::Form { files, .. } = self {
            if files.len() > 0 {
                let mut _form = multipart::Form::new();
                todo!()
            } else {
                None
            }
        } else {
            None
        }
    }
}

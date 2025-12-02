use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde::ser::SerializeMap;
use std::borrow::Cow;
use std::collections::HashMap;

pub struct ProblemDetails {
    status: StatusCode,
    details: HashMap<Cow<'static, str>, String>,
}

impl Serialize for ProblemDetails {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.details.len() + 1))?;
        map.serialize_entry("status", &self.status.as_u16())?;
        for (k, v) in &self.details {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl ProblemDetails {
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            details: HashMap::new(),
        }
    }

    pub fn with_type(mut self, value: &str) -> Self {
        self.details.insert(Cow::Borrowed("type"), value.to_string());
        self
    }

    pub fn with_detail(mut self, key: &str, value: &str) -> Self {
        self.details.insert(Cow::Owned(key.to_string()), value.to_string());
        self
    }

    pub fn with_title(mut self, value: &str) -> Self {
        self.details.insert(Cow::Borrowed("title"), value.to_string());
        self
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        (self.status, Json(self)).into_response()
    }
}

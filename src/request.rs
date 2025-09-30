use std::str::FromStr;

use axum::{body::Body, extract::Request, http::method};
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestWarpper {
    request_id: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    method: String,
}

impl RequestWarpper {
    pub async fn from_request(request_id: &str, request: Request) -> Self {
        let url = request.uri().to_string();
        let headers = request
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();
        let method = request.method().to_string();
        let mut body_stream = request.into_body().into_data_stream();

        let mut base64_body = String::new();
        if let Some(Ok(body)) = body_stream.next().await {
            let en = general_purpose::STANDARD.encode(body);
            base64_body.push_str(&en);
        }
        Self {
            request_id: request_id.to_string(),
            url,
            headers,
            method,
            body: Some(base64_body),
        }
    }

    pub fn to_request(&self) -> anyhow::Result<Request<Body>> {
        let mut builder = Request::builder()
            .uri(self.url.clone())
            .method(method::Method::from_str(&self.method)?);

        for (header, value) in &self.headers {
            builder = builder.header(header, value);
        }

        Ok(if let Some(body) = &self.body {
            let de = general_purpose::STANDARD.decode(body)?;
            builder.body(Body::from(de))?
        } else {
            builder.body(Body::empty())?
        })
    }
}

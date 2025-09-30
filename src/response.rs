use axum::{body::Body, response::Response};
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt as _;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWarpper {
    request_id: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl ResponseWarpper {
    pub async fn from_response(request_id: &str, response: Response) -> Self {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();
        let mut body_stream = response.into_body().into_data_stream();
        let mut base64_body = String::new();
        if let Some(Ok(body)) = body_stream.next().await {
            let en = general_purpose::STANDARD.encode(body);
            base64_body.push_str(&en);
        }

        Self {
            request_id: String::from(request_id),
            status: status,
            headers: headers,
            body: Some(base64_body),
        }
    }

    pub fn new_bad_response(request_id: &str) -> Self {
        Self {
            request_id: String::from(request_id),
            status: StatusCode::BAD_REQUEST.as_u16(),
            headers: vec![],
            body: None,
        }
    }

    pub fn to_response(&self) -> Response {
        let mut builder = Response::builder().status(self.status);

        for (header, value) in &self.headers {
            builder = builder.header(header, value);
        }

        builder
            .body(Body::from(self.body.clone().unwrap()))
            .unwrap()
    }
}

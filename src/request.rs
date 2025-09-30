use axum::{body::Body, extract::Request};
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestWarpper {
    request_id: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl RequestWarpper {
    pub async fn from_request(request: Request) -> Self {
        let request_id = request.extensions().get::<String>().unwrap().clone();
        let url = request.uri().to_string();
        let headers = request
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();

        let mut body_stream = request.into_body().into_data_stream();

        let mut base64_body = String::new();
        if let Some(Ok(body)) = body_stream.next().await {
            let en = general_purpose::STANDARD.encode(body);
            base64_body.push_str(&en);
        }
        Self {
            request_id,
            url,
            headers,
            body: Some(base64_body),
        }
    }

    pub fn to_request(&self) -> Request<Body> {
        let mut builder = Request::builder().uri(self.url.clone());

        for (header, value) in &self.headers {
            builder = builder.header(header, value);
        }

        builder
            .body(Body::from(self.body.clone().unwrap()))
            .unwrap()
    }
}

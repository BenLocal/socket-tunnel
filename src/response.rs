use axum::{body::Body, response::Response};
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt as _;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};

use crate::stream::read_all_data_fold;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWarpper {
    request_id: String,
    tunnel_id: Option<String>,
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl ResponseWarpper {
    pub fn tunnel_id(&self) -> Option<&str> {
        self.tunnel_id.as_deref()
    }

    pub async fn from_response(
        request_id: String,
        tunnel_id: Option<String>,
        response: Response,
    ) -> Self {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();
        let body_stream = response.into_body().into_data_stream();
        let body_tmpl = read_all_data_fold(body_stream).await.unwrap_or_default();

        println!("body:{}", String::from_utf8_lossy(&body_tmpl));
        let en = general_purpose::STANDARD.encode(body_tmpl);

        Self {
            request_id,
            status: status,
            headers: headers,
            body: Some(en),
            tunnel_id,
        }
    }

    pub fn new_bad_response(request_id: String, tunnel_id: Option<String>) -> Self {
        Self {
            request_id,
            status: StatusCode::BAD_REQUEST.as_u16(),
            headers: vec![],
            body: None,
            tunnel_id,
        }
    }

    pub fn to_response(&self) -> anyhow::Result<Response> {
        let mut builder = Response::builder().status(self.status);

        for (header, value) in &self.headers {
            builder = builder.header(header, value);
        }

        Ok(if let Some(body) = &self.body {
            let de = general_purpose::STANDARD.decode(body)?;
            println!("response body: {}", String::from_utf8_lossy(&de));
            builder.body(Body::from(de))?
        } else {
            builder.body(Body::empty())?
        })
    }
}

use async_trait::async_trait;
use axum::{body::Body, http::HeaderValue, response::IntoResponse};
use futures_util::{SinkExt as _, StreamExt as _};
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use pingora::{
    server::{ListenFds, Server, ShutdownWatch},
    services::Service,
};
use socket_tunnel::{request::RequestWarpper, response::ResponseWarpper};
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

const SERVER: &str = "ws://127.0.0.1:3000/tunnel/ws";

const CONNECT_ID: &str = "test";

type Client = hyper_util::client::legacy::Client<HttpConnector, Body>;

fn main() -> anyhow::Result<()> {
    let mut my_server = Server::new(None)?;
    my_server.bootstrap();

    my_server.add_service(BackendService::new(SERVER));
    my_server.run_forever();
}

pub struct BackendService {
    front_ws_url: &'static str,
    client: Client,
}

impl BackendService {
    pub fn new(front_ws_url: &'static str) -> Self {
        let client: Client =
            hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
                .build(HttpConnector::new());
        Self {
            front_ws_url,
            client,
        }
    }

    pub async fn connect_async(&self, mut shutdown: ShutdownWatch) -> anyhow::Result<()> {
        let mut req = self.front_ws_url.into_client_request()?;
        req.headers_mut()
            .insert("X-CONNECT-ID", HeaderValue::from_static(CONNECT_ID));
        let (ws_stream, _) = connect_async(req).await?;
        let (mut sender, mut receiver) = ws_stream.split();

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    break;
                }
                msg = receiver.next() => {
                    if let Some(Ok(msg)) = msg {
                         let req_w: RequestWarpper = match serde_json::from_slice(&msg.into_data()){
                            Ok(a) => a,
                            Err(_) => break
                         };
                        // send to backend service
                        let id = req_w.connect_id().to_string();
                        let tunnel_id = req_w.tunnel_id().map(|x| x.to_string());
                        let resp_w = if let Ok(res) = self.handle(req_w).await {
                            ResponseWarpper::from_response(id,tunnel_id, res.into_response()).await
                        } else {
                            ResponseWarpper::new_bad_response(id,tunnel_id)
                        };
                        let json = serde_json::to_vec(&resp_w)?;
                        sender.send(tokio_tungstenite::tungstenite::Message::Binary(json.into())).await?;
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle(&self, rw: RequestWarpper) -> anyhow::Result<impl IntoResponse> {
        let mut req = rw.to_request()?;
        *req.uri_mut() = "http://www.baidu.com".parse()?;
        req.headers_mut().remove("Host");
        println!("req: {:?}", req);
        let res = self.client.request(req).await?;
        println!("resp: {:?}", res);
        Ok(res.into_response())
    }
}

#[async_trait]
impl Service for BackendService {
    async fn start_service(&mut self, _fds: Option<ListenFds>, shutdown: ShutdownWatch, _: usize) {
        loop {
            if *shutdown.borrow() {
                break;
            }

            let shutdown_clone = shutdown.clone();
            if let Err(e) = self.connect_async(shutdown_clone).await {
                println!("error: {}", e);
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    fn name(&self) -> &str {
        "AdminService"
    }

    fn threads(&self) -> Option<usize> {
        Some(1)
    }
}

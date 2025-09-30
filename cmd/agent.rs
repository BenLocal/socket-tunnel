use async_trait::async_trait;
use axum::{body::Body, response::IntoResponse};
use futures_util::{SinkExt as _, StreamExt as _};
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use pingora::{
    server::{ListenFds, Server, ShutdownWatch},
    services::Service,
};
use socket_tunnel::{request::RequestWarpper, response::ResponseWarpper};
use tokio_tungstenite::connect_async;

const SERVER: &str = "ws://127.0.0.1:3000/tunnel/ws";

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
        let (ws_stream, _) = connect_async(self.front_ws_url).await?;
        let (mut sender, mut receiver) = ws_stream.split();

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    break;
                }
                msg = receiver.next() => {
                    if let Some(Ok(msg)) = msg {
                        // send to backend service
                        let resp_w = if let Ok(res) = self.handle(msg).await {
                            ResponseWarpper::from_response("", res.into_response()).await
                        } else {
                            ResponseWarpper::new_bad_response("")
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

    async fn handle(
        &self,
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> anyhow::Result<impl IntoResponse> {
        let req_w: RequestWarpper = serde_json::from_slice(&msg.into_data())?;
        let req = req_w.to_request()?;
        let res = self.client.request(req).await?;
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

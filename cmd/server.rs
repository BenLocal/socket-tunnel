use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Router, response::Html, routing::get};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt as _};
use hyper::HeaderMap;
use socket_tunnel::request::RequestWarpper;
use socket_tunnel::response::ResponseWarpper;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let cancel = CancellationToken::new();

    let cancel_clone = cancel.clone();
    tokio::spawn(async move { start_websocket_server(cancel_clone).await });

    tokio::signal::ctrl_c().await?;
    cancel.cancel();
    Ok(())
}

pub struct WebsocketConnectState {
    pub connects: RwLock<HashMap<String, WebsocketConnect>>,
    pub message: RwLock<HashMap<String, WebsocketTunnelRequest>>,
}

pub struct WebsocketTunnelRequest {
    pub id: String,
    pub on_response: tokio::sync::oneshot::Sender<Bytes>,
}

impl WebsocketTunnelRequest {
    pub fn new(on_response: tokio::sync::oneshot::Sender<Bytes>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self { id, on_response }
    }

    pub fn request_id(&self) -> &str {
        return &self.id;
    }
}

pub struct WebsocketConnect {
    pub id: String,
    // pub who: SocketAddr,
    pub socket: SplitSink<WebSocket, Message>,
}

pub async fn start_websocket_server(shutdown: CancellationToken) -> anyhow::Result<()> {
    let state = Arc::new(WebsocketConnectState {
        connects: RwLock::new(HashMap::new()),
        message: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/tunnel/healthz", get(handler))
        .route("/tunnel/ws", any(ws_handler))
        .fallback(ws_forward)
        .with_state(state);

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    info!("admin server listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown.cancelled() => {
                  info!("admin server shutdown");
                },
            }
        })
        .await?;

    Ok(())
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

async fn ws_forward(
    State(state): State<Arc<WebsocketConnectState>>,
    req: Request,
) -> Result<Response, StatusCode> {
    let mut connects = state.connects.write().await;
    let (tx, rx) = tokio::sync::oneshot::channel::<Bytes>();

    let mut w = RequestWarpper::from_request(req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let res = match connects.get_mut(w.connect_id()) {
        Some(connect) => {
            let tunnel = WebsocketTunnelRequest::new(tx);
            let tunnel_id = tunnel.id.to_string();
            state
                .message
                .write()
                .await
                .insert(tunnel.id.clone(), tunnel);
            w.set_tunnel_id(&tunnel_id);
            let json = serde_json::to_string(&w).map_err(|_| StatusCode::BAD_REQUEST)?;
            info!("request: {}", json);
            connect
                .socket
                .send(Message::Binary(json.into()))
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let resp = rx.await.map_err(|_| StatusCode::BAD_REQUEST)?;
            serde_json::from_slice::<ResponseWarpper>(&resp).map_err(|_| StatusCode::BAD_REQUEST)?
        }
        None => return Err(StatusCode::BAD_REQUEST),
    };

    Ok(res.to_response().map_err(|_| StatusCode::BAD_REQUEST)?)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    //ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<WebsocketConnectState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, headers))
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<WebsocketConnectState>,
    headers: HeaderMap,
    //who: SocketAddr,
) {
    let id = match headers
        .iter()
        .find(|x| x.0 == "X-CONNECT-ID")
        .map(|v| v.1.to_str().ok())
        .flatten()
    {
        Some(id) => id,
        None => return,
    };
    let (sink, mut stream) = socket.split();

    {
        info!("connect {} connected", id);
        state.connects.write().await.insert(
            id.to_string(),
            WebsocketConnect {
                id: id.to_string(),
                // who,
                socket: sink,
            },
        );
    }

    while let Some(Ok(msg)) = stream.next().await {
        let data = msg.into_data();
        info!(
            "connect {} received: {}",
            id,
            String::from_utf8_lossy(&data)
        );

        let resp = serde_json::from_slice::<ResponseWarpper>(&data);
        if let Ok(resp) = resp {
            let mut rev = state.message.write().await;
            let tunnel_id = resp.tunnel_id();
            if let Some(tunnel_id) = tunnel_id {
                if let Some(tunnel) = rev.remove(tunnel_id) {
                    tunnel.on_response.send(data).unwrap();
                }
            }
        }
    }

    {
        let mut connects = state.connects.write().await;
        if let Some(_) = connects.remove(id) {
            info!("connect {} disconnected", id);
        }
    }
}

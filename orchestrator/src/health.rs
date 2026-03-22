use crate::{AppState, ServerStatus};
use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info};

pub async fn serve_health(
    state: Arc<RwLock<AppState>>,
    port: u16,
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!(port, "health endpoint listening");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                let state = state.clone();
                async move {
                    handle_health(req, state).await
                }
            });

            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                debug!("health connection error: {e}");
            }
        });
    }
}

async fn handle_health(
    _req: Request<hyper::body::Incoming>,
    state: Arc<RwLock<AppState>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let s = state.read().await;

    let (status_code, body) = match s.status {
        ServerStatus::Ready => (StatusCode::OK, r#"{"status":"ok"}"#),
        ServerStatus::Starting => (StatusCode::SERVICE_UNAVAILABLE, r#"{"status":"starting"}"#),
    };

    let response = Response::builder()
        .status(status_code)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    Ok(response)
}

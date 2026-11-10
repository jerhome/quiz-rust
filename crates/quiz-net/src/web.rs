//! Browser transport: static pages and the `/ws` WebSocket (same JSON messages as TCP,
//! one per text frame).

use crate::conn::{serve, Link};
use crate::protocol::{Device, ServerMsg, MAX_WS_FRAME_BYTES};
use crate::server::Cmd;
use crate::HeartbeatConfig;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::header::{
    ALLOW, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, unbounded_channel, UnboundedReceiver};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

// A browser that cannot drain this many messages is cut instead of buffered indefinitely.
const MAX_QUEUED_OUT: usize = 256;

// Safari does not match `'self'` against ws:// in connect-src, hence the explicit `ws:`.
const CSP: &str = "default-src 'self'; connect-src 'self' ws:; img-src 'self' data:; \
                   style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; \
                   frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
const PING_TEXT: &str = "{\"type\":\"Ping\"}";

/// Static files served to browsers, keyed by path (`"index.html"` answers `/`).
#[derive(Clone, Default)]
pub struct WebAssets {
    files: HashMap<String, (&'static [u8], &'static str)>,
}

impl WebAssets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, path: &str, body: &'static [u8], mime: &'static str) -> Self {
        self.files
            .insert(path.trim_start_matches('/').to_string(), (body, mime));
        self
    }
}

#[derive(Clone)]
pub(crate) struct WebState {
    pub assets: Arc<WebAssets>,
    pub cmd: mpsc::UnboundedSender<Cmd>,
    pub stop: watch::Receiver<bool>,
    pub hb: HeartbeatConfig,
}

pub(crate) fn router(state: WebState) -> Router {
    Router::new()
        .route("/ws", get(ws_upgrade))
        .fallback(assets)
        .layer(map_response(security_headers))
        .with_state(state)
}

async fn security_headers(mut resp: Response) -> Response {
    let h = resp.headers_mut();
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    h.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    resp
}

async fn assets(State(st): State<WebState>, method: Method, uri: Uri) -> Response {
    if method != Method::GET {
        let mut r = StatusCode::METHOD_NOT_ALLOWED.into_response();
        r.headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET"));
        return r;
    }
    let path = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        p => p,
    };
    let Some((body, mime)) = st.assets.files.get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut r = Response::new(Body::from(*body));
    let mime = HeaderValue::from_str(mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    r.headers_mut().insert(CONTENT_TYPE, mime);
    r
}

// Blocks pages from other origins from driving a player's session through their browser.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let host = headers.get(HOST).and_then(|h| h.to_str().ok());
    let origin = origin.to_str().ok().and_then(|o| o.split_once("://"));
    matches!((origin, host), (Some((_, a)), Some(h)) if a.eq_ignore_ascii_case(h))
}

// DNS rebinding: a rebound hostname still arrives as `Host: attacker.example`, whereas
// players reach the host by IP (or localhost).
fn host_allowed(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map(|(a, _)| a),
        None => Some(host.rsplit_once(':').map_or(host, |(a, _)| a)),
    };
    name.is_some_and(|n| n.parse::<IpAddr>().is_ok() || n.eq_ignore_ascii_case("localhost"))
}

async fn ws_upgrade(
    State(st): State<WebState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !host_allowed(&headers) || !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_frame_size(MAX_WS_FRAME_BYTES)
        .max_message_size(MAX_WS_FRAME_BYTES)
        .on_upgrade(move |socket| session(socket, st))
}

async fn session(socket: WebSocket, st: WebState) {
    let (in_tx, inbound) = mpsc::channel(32);
    let (out, out_rx) = unbounded_channel();
    tokio::spawn(serve(
        Link {
            inbound,
            out,
            device: Device::Web,
        },
        st.cmd,
        st.stop,
        st.hb,
    ));
    ws_io(socket, in_tx, out_rx, st.hb).await;
}

// Ends when the connection task and the host drop their senders, after flushing what is queued.
async fn ws_io(
    mut socket: WebSocket,
    in_tx: mpsc::Sender<String>,
    mut out_rx: UnboundedReceiver<ServerMsg>,
    hb: HeartbeatConfig,
) {
    let (ping_every, send_timeout) = (hb.interval, hb.idle_timeout);
    let mut ping = tokio::time::interval_at(tokio::time::Instant::now() + ping_every, ping_every);
    loop {
        tokio::select! {
            m = out_rx.recv() => {
                let Some(m) = m else { break };
                if out_rx.len() > MAX_QUEUED_OUT {
                    return;
                }
                let Ok(json) = serde_json::to_string(&m) else { continue };
                if !send(&mut socket, Message::Text(json.into()), send_timeout).await {
                    return;
                }
            }
            _ = ping.tick() => {
                if !send(&mut socket, Message::Text(PING_TEXT.into()), send_timeout).await {
                    return;
                }
            }
            frame = socket.recv() => match frame {
                Some(Ok(Message::Text(t))) => {
                    if in_tx.send(t.as_str().to_owned()).await.is_err() {
                        return;
                    }
                }
                // Protocol-level ping/pong is answered by the library; the application
                // heartbeat is the text Ping/Pong above.
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => return,
            },
        }
    }
    send(&mut socket, Message::Close(None), send_timeout).await;
}

// A peer that stops reading would otherwise block this task (and its permit) forever.
async fn send(socket: &mut WebSocket, m: Message, limit: Duration) -> bool {
    matches!(timeout(limit, socket.send(m)).await, Ok(Ok(())))
}

// Holds the connection permit for as long as the socket lives, including after a WebSocket
// upgrade moves the stream out of the HTTP connection task.
struct Permitted {
    stream: TcpStream,
    _permit: OwnedSemaphorePermit,
}

impl AsyncRead for Permitted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Permitted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Serves HTTP/WebSocket until `stop`; open connections are then shut down gracefully.
pub(crate) async fn serve_http(
    listener: TcpListener,
    app: Router,
    permits: Arc<Semaphore>,
    mut stop: watch::Receiver<bool>,
    header_timeout: Duration,
) {
    loop {
        let stream = tokio::select! {
            _ = stop.changed() => return,
            res = listener.accept() => match res {
                Ok((s, _)) => s,
                Err(e) => {
                    tracing::warn!("web accept: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            continue;
        };
        let io = TokioIo::new(Permitted {
            stream,
            _permit: permit,
        });
        let svc = TowerToHyperService::new(app.clone());
        let mut stop = stop.clone();
        tokio::spawn(async move {
            let conn = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout)
                .serve_connection(io, svc)
                .with_upgrades();
            tokio::pin!(conn);
            tokio::select! {
                _ = conn.as_mut() => return,
                _ = stop.changed() => {}
            }
            conn.as_mut().graceful_shutdown();
            let _ = timeout(Duration::from_secs(2), conn).await;
        });
    }
}

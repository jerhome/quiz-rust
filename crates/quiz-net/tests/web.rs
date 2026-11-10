//! Browser transport: static pages + WebSocket, session resume (TCP and WS).

use futures_util::{SinkExt, StreamExt};
use quiz_core::Question;
use quiz_net::{
    Client, Device, GamePhase, HeartbeatConfig, Host, HostConfig, HostEvent, HostHandle,
    PlayerInfo, ServerMsg, WebAssets, PROTOCOL_VERSION,
};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const T: Duration = Duration::from_secs(3);
const HTML: &[u8] = b"<html>quiz</html>";

fn questions(n: u32) -> Vec<Question> {
    (0..n)
        .map(|id| Question {
            id,
            prompt: format!("Question {id}"),
            choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct: 2,
            explanation: "EXPLICATION_SECRETE".into(),
        })
        .collect()
}

fn config() -> HostConfig {
    HostConfig {
        port: 0,
        announce: false,
        time_limit: None,
        web_port: Some(0),
        web_assets: WebAssets::new()
            .with("index.html", HTML, "text/html; charset=utf-8")
            .with("app.js", b"let x=1;", "text/javascript"),
        ..HostConfig::default()
    }
}

async fn host_with(cfg: HostConfig) -> HostHandle {
    Host::start(cfg).await.unwrap()
}

async fn host_event_until<F: Fn(&HostEvent) -> bool>(h: &mut HostHandle, f: F) -> HostEvent {
    loop {
        let ev = timeout(T, h.events.recv()).await.unwrap().unwrap();
        if f(&ev) {
            return ev;
        }
    }
}

fn player<'a>(p: &'a [PlayerInfo], name: &str) -> Option<&'a PlayerInfo> {
    p.iter().find(|x| x.name == name)
}

async fn wait_ghost(h: &mut HostHandle, name: &str) {
    host_event_until(h, |e| {
        matches!(e, HostEvent::LobbyChanged(p) if player(p, name).is_some_and(|x| !x.connected))
    })
    .await;
}

async fn wait_progress(h: &mut HostHandle, n: usize) {
    host_event_until(
        h,
        |e| matches!(e, HostEvent::AnswerProgress { answered, .. } if *answered == n),
    )
    .await;
}

async fn http(port: u16, req: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    timeout(T, s.read_to_end(&mut out)).await.unwrap().unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

struct Ws {
    s: WebSocketStream<MaybeTlsStream<TcpStream>>,
    raw: Vec<String>,
}

impl Ws {
    async fn connect(port: u16) -> Self {
        let (s, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .unwrap();
        Self { s, raw: vec![] }
    }

    async fn send(&mut self, v: serde_json::Value) {
        self.s.send(Message::text(v.to_string())).await.unwrap();
    }

    /// Joins and returns (ws, player_id, resume_token).
    async fn join(port: u16, name: &str, resume: Option<&str>) -> (Self, u32, String) {
        Self::join_version(port, name, resume, PROTOCOL_VERSION)
            .await
            .unwrap_or_else(|m| panic!("refused: {m:?}"))
    }

    async fn join_version(
        port: u16,
        name: &str,
        resume: Option<&str>,
        version: u32,
    ) -> Result<(Self, u32, String), ServerMsg> {
        let mut ws = Self::connect(port).await;
        ws.send(json!({"type":"Join","name":name,"protocol_version":version,"resume":resume}))
            .await;
        match ws.next().await.expect("closed before Welcome") {
            ServerMsg::Welcome {
                player_id,
                resume_token,
                ..
            } => Ok((ws, player_id, resume_token)),
            other => Err(other),
        }
    }

    /// Answers application pings like a real page does; `None` once the socket is closed.
    async fn next(&mut self) -> Option<ServerMsg> {
        loop {
            let m = timeout(T, self.s.next()).await.expect("timeout")?.ok()?;
            let Message::Text(t) = m else {
                if matches!(m, Message::Close(_)) {
                    return None;
                }
                continue;
            };
            let t = t.as_str().to_owned();
            if t.contains("\"Ping\"") {
                self.send(json!({"type":"Pong"})).await;
                continue;
            }
            self.raw.push(t.clone());
            return Some(serde_json::from_str(&t).unwrap());
        }
    }

    async fn until<F: Fn(&ServerMsg) -> bool>(&mut self, f: F) -> ServerMsg {
        loop {
            let m = self.next().await.expect("closed");
            if f(&m) {
                return m;
            }
        }
    }

    async fn answer(&mut self, question_id: u32, choice: usize) {
        self.send(json!({"type":"Answer","question_id":question_id,"choice":choice}))
            .await;
    }
}

fn is_question(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::Question(_))
}
fn is_reveal(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::Reveal { .. })
}

#[tokio::test]
async fn http_serves_assets_with_headers_and_refuses_others() {
    let h = host_with(config()).await;
    let port = h.web_port().expect("web port");
    let r = http(
        port,
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let low = r.to_lowercase();
    assert!(
        low.contains("content-type: text/html; charset=utf-8"),
        "{r}"
    );
    assert!(low.contains("cache-control: no-cache"), "{r}");
    assert!(low.contains("x-content-type-options: nosniff"), "{r}");
    assert!(
        low.contains("content-security-policy: default-src 'self'"),
        "{r}"
    );
    assert!(low.contains("connect-src 'self' ws:"), "{r}");
    assert!(r.ends_with("<html>quiz</html>"), "{r}");

    let r = http(
        port,
        "GET /app.js HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        r.to_lowercase().contains("content-type: text/javascript"),
        "{r}"
    );

    let r = http(
        port,
        "GET /nope HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 404"), "{r}");
    assert!(
        r.to_lowercase().contains("x-content-type-options: nosniff"),
        "{r}"
    );

    let r = http(
        port,
        "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 405"), "{r}");
}

#[tokio::test]
async fn mixed_game_tcp_and_ws_scores_and_no_leak() {
    let mut h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let tcp = Client::connect(("127.0.0.1", h.port()), "Alice", None)
        .await
        .unwrap();
    let (mut ws, _, _) = Ws::join(port, "Web", None).await;
    let lobby = ws
        .until(|m| matches!(m, ServerMsg::Lobby { players, .. } if players.len() == 3))
        .await;
    let ServerMsg::Lobby { players, .. } = lobby else {
        unreachable!()
    };
    assert_eq!(player(&players, "Web").unwrap().device, Device::Web);
    assert_eq!(player(&players, "Alice").unwrap().device, Device::App);
    assert!(players.iter().all(|p| p.connected));

    h.start_game(questions(1), None);
    let ServerMsg::Question(q) = ws.until(is_question).await else {
        unreachable!()
    };
    ws.answer(q.id, 2).await;
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::AnswerProgress { answered, .. } if *answered == 1),
    )
    .await;
    tcp.answer(q.id, 2);
    wait_progress(&mut h, 2).await;
    h.host_answer(0);
    let ServerMsg::Reveal { scores, .. } = ws.until(is_reveal).await else {
        unreachable!()
    };
    let web = scores.iter().find(|s| s.name == "Web").unwrap();
    let alice = scores.iter().find(|s| s.name == "Alice").unwrap();
    assert_eq!((web.score, alice.score), (1000, 900));
    h.next_question();
    let ServerMsg::End { ranking } = ws.until(|m| matches!(m, ServerMsg::End { .. })).await else {
        unreachable!()
    };
    assert_eq!(ranking[0].name, "Web");
    let questions_seen: Vec<&String> = ws
        .raw
        .iter()
        .filter(|r| r.contains("\"Question\""))
        .collect();
    assert!(!questions_seen.is_empty());
    for r in questions_seen {
        assert!(
            !r.contains("correct") && !r.contains("EXPLICATION_SECRETE"),
            "{r}"
        );
    }
}

#[tokio::test]
async fn ws_resume_during_question_keeps_id_score_and_first_answer() {
    let mut h = host_with(HostConfig {
        time_limit: Some(Duration::from_secs(5)),
        ..config()
    })
    .await;
    let port = h.web_port().unwrap();
    let bob = Client::connect(("127.0.0.1", h.port()), "Bob", None)
        .await
        .unwrap();
    let (mut ws, id, token) = Ws::join(port, "Alice", None).await;
    h.start_game(questions(2), Some(Duration::from_secs(5)));
    let ServerMsg::Question(q) = ws.until(is_question).await else {
        unreachable!()
    };
    ws.answer(q.id, 2).await;
    wait_progress(&mut h, 1).await;
    drop(ws); // abrupt: no Close frame
    wait_ghost(&mut h, "Alice").await;

    let (mut ws, id2, token2) = Ws::join(port, "Alice", Some(&token)).await;
    assert_eq!(id2, id);
    assert_ne!(token2, token, "token rotated");
    let ServerMsg::Lobby { players, phase } =
        ws.until(|m| matches!(m, ServerMsg::Lobby { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(phase, GamePhase::Question);
    assert!(player(&players, "Alice").unwrap().connected);
    let ServerMsg::Question(q2) = ws.until(is_question).await else {
        unreachable!()
    };
    assert_eq!(q2.id, q.id);
    assert!(q2.remaining_ms.unwrap() <= 5000);

    ws.answer(q.id, 0).await; // second answer to the same question: ignored
    h.host_answer(1);
    bob.answer(q.id, 1);
    let ServerMsg::Reveal { scores, .. } = ws.until(is_reveal).await else {
        unreachable!()
    };
    let a = scores.iter().find(|s| s.player_id == id).unwrap();
    assert_eq!(a.choice, Some(2));
    assert!(a.score >= 500);
}

#[tokio::test]
async fn ws_resume_in_lobby_and_after_reveal() {
    let mut h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let (ws, id, token) = Ws::join(port, "Alice", None).await;
    drop(ws);
    wait_ghost(&mut h, "Alice").await;
    let (mut ws, id2, token) = Ws::join(port, "Alice", Some(&token)).await;
    assert_eq!(id2, id);
    let ServerMsg::Lobby { players, phase } =
        ws.until(|m| matches!(m, ServerMsg::Lobby { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(phase, GamePhase::Lobby);
    assert!(player(&players, "Alice").unwrap().connected);

    h.start_game(questions(2), None);
    let ServerMsg::Question(q) = ws.until(is_question).await else {
        unreachable!()
    };
    ws.answer(q.id, 2).await;
    wait_progress(&mut h, 1).await;
    h.host_answer(2);
    ws.until(is_reveal).await;
    drop(ws);
    wait_ghost(&mut h, "Alice").await;
    let (mut ws, id3, _) = Ws::join(port, "Alice", Some(&token)).await;
    assert_eq!(id3, id);
    let ServerMsg::Reveal {
        correct, scores, ..
    } = ws.until(is_reveal).await
    else {
        unreachable!()
    };
    assert_eq!(correct, 2);
    assert_eq!(
        scores.iter().find(|s| s.player_id == id).unwrap().score,
        1000
    );
}

#[tokio::test]
async fn disconnected_player_is_not_awaited_for_reveal() {
    let mut h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let (mut bob, _, _) = Ws::join(port, "Bob", None).await;
    let (alice, alice_id, _) = Ws::join(port, "Alice", None).await;
    h.start_game(questions(1), None);
    let ServerMsg::Question(q) = bob.until(is_question).await else {
        unreachable!()
    };
    drop(alice);
    wait_ghost(&mut h, "Alice").await;
    bob.answer(q.id, 2).await;
    h.host_answer(2);
    let ServerMsg::Reveal { scores, .. } = bob.until(is_reveal).await else {
        unreachable!()
    };
    let a = scores.iter().find(|s| s.player_id == alice_id).unwrap();
    assert_eq!((a.score, a.choice), (0, None));
}

#[tokio::test]
async fn expired_token_gives_a_new_player() {
    let mut h = host_with(HostConfig {
        resume_grace: Duration::from_millis(300),
        ..config()
    })
    .await;
    let port = h.web_port().unwrap();
    let (ws, id, token) = Ws::join(port, "Alice", None).await;
    drop(ws);
    wait_ghost(&mut h, "Alice").await;
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if player(p, "Alice").is_none()),
    )
    .await;
    let (_ws, id2, _) = Ws::join(port, "Alice", Some(&token)).await;
    assert_ne!(id2, id);
}

#[tokio::test]
async fn same_token_twice_closes_the_older_connection() {
    let h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let (mut old, id, token) = Ws::join(port, "Alice", None).await;
    let (_new, id2, _) = Ws::join(port, "Alice", Some(&token)).await;
    assert_eq!(id2, id);
    loop {
        if old.next().await.is_none() {
            break;
        }
    }
}

#[tokio::test]
async fn tcp_client_can_resume() {
    let mut h = host_with(config()).await;
    let a = Client::connect(("127.0.0.1", h.port()), "Alice", None)
        .await
        .unwrap();
    let (id, token) = (a.player_id(), a.resume_token().to_string());
    drop(a);
    wait_ghost(&mut h, "Alice").await;
    let b = Client::connect_resume(
        ("127.0.0.1", h.port()),
        "Alice",
        Some(token.clone()),
        None,
        HeartbeatConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(b.player_id(), id);
    assert_ne!(b.resume_token(), token);
}

#[tokio::test]
async fn oversized_frame_closes_connection() {
    let h = host_with(config()).await;
    let (mut ws, _, _) = Ws::join(h.web_port().unwrap(), "Alice", None).await;
    let _ =
        ws.s.send(Message::text(
            json!({"type":"Join","name":"x".repeat(20_000)}).to_string(),
        ))
        .await;
    loop {
        if ws.next().await.is_none() {
            break;
        }
    }
}

#[tokio::test]
async fn ws_version_mismatch_is_reported() {
    let h = host_with(config()).await;
    let r = Ws::join_version(h.web_port().unwrap(), "Zed", None, PROTOCOL_VERSION + 1).await;
    let Err(ServerMsg::Error { code, .. }) = r else {
        panic!("expected error");
    };
    assert_eq!(code, quiz_net::ErrorCode::VersionMismatch);
}

#[tokio::test]
async fn busy_web_port_does_not_stop_tcp() {
    let busy = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let h = host_with(HostConfig {
        web_port: Some(busy.local_addr().unwrap().port()),
        ..config()
    })
    .await;
    assert_eq!(h.web_port(), None);
    Client::connect(("127.0.0.1", h.port()), "Alice", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn silent_ws_client_is_detected_by_heartbeat() {
    let mut h = host_with(HostConfig {
        heartbeat: HeartbeatConfig {
            interval: Duration::from_millis(100),
            idle_timeout: Duration::from_millis(400),
        },
        ..config()
    })
    .await;
    let (_mute, _, _) = Ws::join(h.web_port().unwrap(), "Alice", None).await;
    wait_ghost(&mut h, "Alice").await;
}

#[tokio::test]
async fn foreign_origin_is_refused() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let h = host_with(config()).await;
    let mut req = format!("ws://127.0.0.1:{}/ws", h.web_port().unwrap())
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Origin", "http://evil.example".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(req).await.is_err());
}

async fn http_head(port: u16, req: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = timeout(T, s.read(&mut buf)).await.unwrap().unwrap();
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn upgrade_req(host: Option<&str>, origin: Option<&str>) -> String {
    let mut r = String::from("GET /ws HTTP/1.1\r\n");
    if let Some(h) = host {
        r += &format!("Host: {h}\r\n");
    }
    if let Some(o) = origin {
        r += &format!("Origin: {o}\r\n");
    }
    r + "Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
}

#[tokio::test]
async fn ws_requires_ip_or_localhost_host_header() {
    let h = host_with(config()).await;
    let port = h.web_port().unwrap();
    for (host, origin, ok) in [
        (Some(format!("127.0.0.1:{port}")), None, true),
        (Some(format!("LocalHost:{port}")), None, true),
        (Some(format!("[::1]:{port}")), None, true),
        (Some("192.168.1.20".to_string()), None, true),
        (
            Some(format!("LOCALHOST:{port}")),
            Some(format!("http://localhost:{port}")),
            true,
        ),
        (Some(format!("evil.example:{port}")), None, false),
        (
            Some(format!("evil.example:{port}")),
            Some(format!("http://evil.example:{port}")),
            false,
        ),
        (Some(format!("127.0.0.1.evil.example:{port}")), None, false),
        (Some("localhost.evil.example".to_string()), None, false),
        (None, None, false),
        (
            Some(format!("127.0.0.1:{port}")),
            Some("null".to_string()),
            false,
        ),
        (
            Some(format!("127.0.0.1:{port}")),
            Some("http://evil.example".to_string()),
            false,
        ),
    ] {
        let r = http_head(port, &upgrade_req(host.as_deref(), origin.as_deref())).await;
        let want = if ok { "HTTP/1.1 101" } else { "HTTP/1.1 403" };
        assert!(r.starts_with(want), "{host:?} {origin:?}: {r}");
    }
}

#[tokio::test]
async fn security_headers_are_complete() {
    let h = host_with(config()).await;
    let r = http(
        h.web_port().unwrap(),
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await
    .to_lowercase();
    for want in [
        "frame-ancestors 'none'",
        "base-uri 'none'",
        "form-action 'none'",
        "connect-src 'self' ws:",
        "referrer-policy: no-referrer",
    ] {
        assert!(r.contains(want), "{want}: {r}");
    }
}

#[tokio::test]
async fn token_replay_after_two_renewals_gives_a_new_player() {
    let h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let (a, id, t1) = Ws::join(port, "Alice", None).await;
    let (b, id2, t2) = Ws::join(port, "Alice", Some(&t1)).await;
    assert_eq!(id2, id);
    let (c, id3, t3) = Ws::join(port, "Alice", Some(&t2)).await;
    assert_eq!(id3, id);
    assert_ne!(t1, t3);
    drop((a, b, c));
    let (_d, id4, _) = Ws::join(port, "Mallory", Some(&t1)).await;
    assert_ne!(id4, id);
}

async fn raw_ws(port: u16) -> TcpStream {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(upgrade_req(Some(&format!("127.0.0.1:{port}")), None).as_bytes())
        .await
        .unwrap();
    let mut buf = vec![0u8; 1024];
    let n = timeout(T, s.read(&mut buf)).await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 101"));
    s
}

// Client frames must be masked; an all-zero key leaves the payload unchanged.
fn ws_frame(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![if fin { 0x80 } else { 0 } | opcode, 0x80 | 126];
    f.extend((payload.len() as u16).to_be_bytes());
    f.extend([0u8; 4]);
    f.extend(payload);
    f
}

async fn assert_closed(s: &mut TcpStream) {
    let mut buf = vec![0u8; 4096];
    loop {
        match timeout(T, s.read(&mut buf)).await.expect("not closed") {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

async fn assert_host_alive(h: &HostHandle) {
    Client::connect(("127.0.0.1", h.port()), "Alive", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn binary_and_fragmented_oversize_messages_close_the_socket() {
    let h = host_with(config()).await;
    let port = h.web_port().unwrap();
    let mut s = raw_ws(port).await;
    s.write_all(&ws_frame(2, true, &vec![7u8; 9000]))
        .await
        .unwrap();
    assert_closed(&mut s).await;

    let mut s = raw_ws(port).await;
    s.write_all(&ws_frame(1, false, &vec![b'a'; 5000]))
        .await
        .unwrap();
    s.write_all(&ws_frame(0, true, &vec![b'a'; 5000]))
        .await
        .unwrap();
    assert_closed(&mut s).await;

    let mut s = raw_ws(port).await;
    s.write_all(&ws_frame(2, true, b"small")).await.unwrap();
    assert_closed(&mut s).await;
    assert_host_alive(&h).await;
}

#[tokio::test]
async fn silent_connections_are_closed_and_free_their_permit() {
    let h = host_with(HostConfig {
        max_connections: 2,
        ..config()
    })
    .await;
    let (port, wport) = (h.port(), h.web_port().unwrap());
    let mut a = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let b = TcpStream::connect(("127.0.0.1", wport)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Over the limit (shared between TCP and web): dropped at once.
    let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    assert_closed(&mut c).await;
    let mut buf = [0u8; 64];
    let started = std::time::Instant::now();
    // The silent TCP peer never sent Join: closed by the handshake timeout (~5 s).
    timeout(Duration::from_secs(8), async {
        while matches!(a.read(&mut buf).await, Ok(n) if n > 0) {}
    })
    .await
    .expect("handshake timeout did not close the connection");
    assert!(started.elapsed() > Duration::from_secs(3));
    drop(b);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_host_alive(&h).await;
}

#[tokio::test]
async fn idle_http_connection_is_closed_by_timeout_and_on_shutdown() {
    let h = host_with(HostConfig {
        http_timeout: Duration::from_millis(500),
        ..config()
    })
    .await;
    let mut idle = TcpStream::connect(("127.0.0.1", h.web_port().unwrap()))
        .await
        .unwrap();
    assert_closed(&mut idle).await;

    let h2 = host_with(config()).await;
    let mut idle = TcpStream::connect(("127.0.0.1", h2.web_port().unwrap()))
        .await
        .unwrap();
    let (mut ws, _, _) = Ws::join(h2.web_port().unwrap(), "Alice", None).await;
    h2.shutdown();
    assert_closed(&mut idle).await;
    while ws.next().await.is_some() {}
}

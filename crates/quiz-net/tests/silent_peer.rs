//! Peers that go silent without FIN/RST (sleeping laptop, VPN dropping the flow): the
//! sockets stay open, nothing is read or written.
use quiz_core::Question;
use quiz_net::{
    Client, ClientEvent, HeartbeatConfig, Host, HostConfig, HostEvent, ServerMsg, PROTOCOL_VERSION,
};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const HB: HeartbeatConfig = HeartbeatConfig {
    interval: Duration::from_millis(100),
    idle_timeout: Duration::from_millis(500),
};

fn questions() -> Vec<Question> {
    vec![Question {
        id: 0,
        prompt: "Q".into(),
        choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        correct: 2,
        explanation: "e".into(),
    }]
}

#[tokio::test]
async fn host_drops_a_silent_player_and_reveals_in_time() {
    let mut h = Host::start(HostConfig {
        port: 0,
        announce: false,
        time_limit: None,
        heartbeat: HB,
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let mut alice = Client::connect_with(("127.0.0.1", h.port()), "Alice", None, HB)
        .await
        .unwrap();

    // Ghost joins, then never reads nor writes again, but keeps the socket open.
    let mut ghost = TcpStream::connect(("127.0.0.1", h.port())).await.unwrap();
    let join = format!(
        "{{\"type\":\"Join\",\"name\":\"Ghost\",\"protocol_version\":{PROTOCOL_VERSION}}}\n"
    );
    ghost.write_all(join.as_bytes()).await.unwrap();
    let mut ghost = BufReader::new(ghost);
    let mut welcome = String::new();
    ghost.read_line(&mut welcome).await.unwrap();
    assert!(welcome.contains("Welcome"), "{welcome}");

    h.start_game(questions(), None);
    loop {
        let m = timeout(Duration::from_secs(3), alice.events.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(m, ClientEvent::Message(ServerMsg::Question(_))) {
            break;
        }
    }
    alice.answer(0, 2);
    h.host_answer(2);

    let t0 = Instant::now();
    let revealed = timeout(Duration::from_secs(3), async {
        loop {
            if matches!(h.events.recv().await, Some(HostEvent::Revealed(_))) {
                break;
            }
        }
    })
    .await;
    assert!(
        revealed.is_ok(),
        "the silent player is still awaited after {:?}",
        t0.elapsed()
    );
    drop(ghost);
}

#[tokio::test]
async fn client_detects_a_host_that_went_silent() {
    // Fake host: completes the handshake, then stays connected but mute.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let fake = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        let mut line = String::new();
        r.read_line(&mut line).await.unwrap();
        let welcome = format!(
            "{{\"type\":\"Welcome\",\"protocol_version\":{PROTOCOL_VERSION},\"player_id\":1,\"resume_token\":\"x\"}}\n"
        );
        w.write_all(welcome.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
        drop((r, w));
    });
    let mut c = Client::connect_with(("127.0.0.1", port), "Bob", None, HB)
        .await
        .unwrap();
    let t0 = Instant::now();
    let got = timeout(Duration::from_secs(3), async {
        loop {
            if let Some(ClientEvent::Disconnected) = c.events.recv().await {
                break;
            }
        }
    })
    .await;
    assert!(
        got.is_ok(),
        "host loss not detected after {:?}",
        t0.elapsed()
    );
    fake.abort();
}

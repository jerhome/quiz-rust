//! Load/chaos: full room (host + 7), random answer delays, abrupt crash, leave, late joins,
//! refused 9th, two games (limited then unlimited with force_reveal). All awaits are bounded.
use quiz_core::Question;
use quiz_net::{
    Client, ClientEvent, ClientHandle, ErrorCode, Host, HostConfig, HostEvent, HostHandle,
    NetError, RankEntry, ScoreEntry, ServerMsg, MAX_PLAYERS, PROTOCOL_VERSION,
};
use std::net::{IpAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{sleep, sleep_until, timeout, Instant};

const T: Duration = Duration::from_secs(10);

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn questions(n: u32) -> Vec<Question> {
    (0..n)
        .map(|id| Question {
            id,
            prompt: format!("Q{id}"),
            choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct: 2,
            explanation: "e".into(),
        })
        .collect()
}

fn target_ip() -> IpAddr {
    let ip = UdpSocket::bind("0.0.0.0:0").ok().and_then(|s| {
        s.connect("192.0.2.1:9").ok()?;
        Some(s.local_addr().ok()?.ip())
    });
    ip.filter(|i| !i.is_loopback())
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap())
}

#[derive(Default)]
struct Log {
    name: String,
    reveals: Vec<(usize, u32, Vec<ScoreEntry>)>,
    ends: Vec<(usize, Vec<RankEntry>)>,
    left: bool,
    disconnected: bool,
}

async fn run_client(
    mut c: ClientHandle,
    name: &str,
    seed: u64,
    max_delay_ms: u64,
    mute: Arc<AtomicBool>,
    leave_at_q1: bool,
) -> Log {
    let mut rng = Rng(seed | 1);
    let mut log = Log {
        name: name.into(),
        ..Log::default()
    };
    let mut game = 0usize;
    let mut pending: Option<(Instant, u32, usize)> = None;
    loop {
        let when = pending.map(|p| p.0);
        tokio::select! {
            ev = c.events.recv() => match ev {
                None | Some(ClientEvent::Disconnected) => { log.disconnected = true; break; }
                Some(ClientEvent::Message(m)) => match m {
                    ServerMsg::Question(q) => {
                        if leave_at_q1 && game == 0 && q.index == 1 && !log.left {
                            log.left = true;
                            c.disconnect();
                            pending = None;
                        } else if !log.left && !mute.load(Ordering::SeqCst) {
                            let delay = rng.next() % (max_delay_ms + 1);
                            let choice = if rng.next() % 100 < 60 { 2 } else { (rng.next() % 4) as usize };
                            pending = Some((Instant::now() + Duration::from_millis(delay), q.id, choice));
                        } else {
                            pending = None;
                        }
                    }
                    ServerMsg::Reveal { question_id, scores, .. } => {
                        pending = None;
                        log.reveals.push((game, question_id, scores));
                    }
                    ServerMsg::End { ranking } => { pending = None; log.ends.push((game, ranking)); }
                    ServerMsg::BackToLobby => game += 1,
                    _ => {}
                }
            },
            _ = async { match when { Some(t) => sleep_until(t).await, None => std::future::pending().await } } => {
                let (_, qid, ch) = pending.take().unwrap();
                c.answer(qid, ch);
            }
        }
    }
    log
}

async fn ev_until<F: Fn(&HostEvent) -> bool>(h: &mut HostHandle, f: F) -> HostEvent {
    loop {
        let ev = timeout(T, h.events.recv())
            .await
            .expect("host event timeout (stuck)")
            .expect("host event channel closed (host panicked?)");
        if f(&ev) {
            return ev;
        }
    }
}

async fn connect(ip: IpAddr, port: u16, name: &str) -> Result<ClientHandle, NetError> {
    Client::connect((ip, port), name, None).await
}

/// Raw client that RSTs the connection (linger 0) once it has seen a question.
async fn spawn_crasher(ip: IpAddr, port: u16, name: &str) {
    let mut s = TcpStream::connect((ip, port)).await.unwrap();
    let join = format!(
        "{{\"type\":\"Join\",\"name\":\"{name}\",\"protocol_version\":{PROTOCOL_VERSION}}}\n"
    );
    s.write_all(join.as_bytes()).await.unwrap();
    let mut r = BufReader::new(s);
    let mut l = String::new();
    r.read_line(&mut l).await.unwrap();
    assert!(l.contains("Welcome"), "{l}");
    tokio::spawn(async move {
        loop {
            l.clear();
            match r.read_line(&mut l).await {
                Ok(n) if n > 0 && l.contains("\"Question\"") => break,
                Ok(n) if n > 0 => {}
                _ => return,
            }
        }
        sleep(Duration::from_millis(100)).await;
        let s = r.into_inner();
        socket2::SockRef::from(&s)
            .set_linger(Some(Duration::ZERO))
            .unwrap();
        drop(s);
    });
}

fn check_log(l: &Log) {
    for (game, qid, scores) in &l.reveals {
        for e in scores {
            assert_eq!(
                e.gained == 0,
                e.choice != Some(2),
                "{}: g{game} q{qid} {e:?}",
                l.name
            );
            assert!(e.score >= e.gained);
        }
        // cumulative within a game
        if let Some((_, _, prev)) = l
            .reveals
            .iter()
            .rev()
            .find(|(g, q, _)| g == game && *q + 1 == *qid)
        {
            for e in scores {
                if let Some(p) = prev.iter().find(|p| p.player_id == e.player_id) {
                    assert_eq!(
                        e.score,
                        p.score + e.gained,
                        "{}: g{game} q{qid} {}",
                        l.name,
                        e.name
                    );
                }
            }
        }
    }
    for (game, ranking) in &l.ends {
        assert!(ranking
            .windows(2)
            .all(|w| w[0].score >= w[1].score && w[0].rank <= w[1].rank));
        assert_eq!(ranking[0].rank, 1);
        let (_, _, last) = l.reveals.iter().rev().find(|(g, _, _)| g == game).unwrap();
        for r in ranking {
            if let Some(s) = last.iter().find(|s| s.player_id == r.player_id) {
                assert_eq!(
                    s.score, r.score,
                    "{}: ranking vs last reveal for {}",
                    l.name, r.name
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_full_room_two_games() {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    eprintln!("chaos seed = {seed}");
    let ip = target_ip();
    let mut h = Host::start(HostConfig {
        port: 0,
        announce: false,
        time_limit: Some(Duration::from_millis(1500)),
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let port = h.port();
    let mute = Arc::new(AtomicBool::new(false));
    let mute_one = Arc::new(AtomicBool::new(false));
    let mut tasks = Vec::new();
    // host + 5 steady + leaver + crasher = MAX_PLAYERS
    for i in 0..(MAX_PLAYERS - 3) {
        let name = format!("S{i}");
        let c = connect(ip, port, &name).await.unwrap();
        // S0 can be muted in game 2 (never answers) to force a manual reveal.
        let m = if i == 0 {
            mute_one.clone()
        } else {
            mute.clone()
        };
        let sd = seed.wrapping_add(i as u64 * 7919);
        tasks.push(tokio::spawn(async move {
            run_client(c, &name, sd, 2000, m, false).await
        }));
    }
    let c = connect(ip, port, "Leaver").await.unwrap();
    let m = mute.clone();
    tasks.push(tokio::spawn(async move {
        run_client(c, "Leaver", seed ^ 0xabc, 500, m, true).await
    }));
    spawn_crasher(ip, port, "Crasher").await;
    ev_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if p.len() == MAX_PLAYERS),
    )
    .await;

    // Ninth connection refused.
    assert!(matches!(
        connect(ip, port, "TooMany").await,
        Err(NetError::Rejected(ErrorCode::LobbyFull))
    ));

    // ---- Game 1: limited to 1.5 s, 4 questions ----
    h.start_game(questions(4), Some(Duration::from_millis(1500)));
    let mut rng = Rng(seed | 1);
    let mut late = Vec::new();
    for q in 0..4 {
        ev_until(&mut h, |e| matches!(e, HostEvent::QuestionStarted(_))).await;
        if q == 0 {
            // Crasher dies ~100 ms in; a slot opens, then a late joiner takes it mid-question.
            sleep(Duration::from_millis(300)).await;
            let c = retry_join(ip, port, "LateQ").await;
            let m = mute.clone();
            late.push(tokio::spawn(async move {
                run_client(c, "LateQ", seed ^ 1, 800, m, false).await
            }));
        }
        sleep(Duration::from_millis(rng.next() % 1200)).await;
        h.host_answer(if rng.next().is_multiple_of(2) { 2 } else { 0 });
        ev_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
        if q == 1 {
            // Leaver left at the start of this question: join during the reveal.
            let c = retry_join(ip, port, "LateR").await;
            let m = mute.clone();
            late.push(tokio::spawn(async move {
                run_client(c, "LateR", seed ^ 2, 800, m, false).await
            }));
            assert!(matches!(
                connect(ip, port, "Overflow").await,
                Err(NetError::Rejected(ErrorCode::LobbyFull))
            ));
        }
        h.next_question();
    }
    ev_until(&mut h, |e| matches!(e, HostEvent::Ended(_))).await;
    h.back_to_lobby();
    ev_until(&mut h, |e| matches!(e, HostEvent::BackToLobby)).await;

    // ---- Game 2: unlimited, manual force_reveal ----
    mute_one.store(true, Ordering::SeqCst);
    h.start_game(questions(3), None);
    ev_until(&mut h, |e| matches!(e, HostEvent::QuestionStarted(_))).await;
    h.host_answer(2);
    // S0 never answers: no automatic reveal.
    sleep(Duration::from_millis(2500)).await;
    while let Ok(ev) = h.events.try_recv() {
        assert!(
            !matches!(ev, HostEvent::Revealed(_)),
            "reveal without S0's answer"
        );
    }
    h.force_reveal();
    ev_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
    mute_one.store(false, Ordering::SeqCst);
    h.next_question();
    ev_until(&mut h, |e| matches!(e, HostEvent::QuestionStarted(_))).await;
    h.host_answer(2);
    // Everybody answers by themselves -> automatic reveal.
    ev_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
    h.next_question();
    ev_until(&mut h, |e| matches!(e, HostEvent::QuestionStarted(_))).await;
    h.host_answer(2);
    ev_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
    h.next_question();
    ev_until(&mut h, |e| matches!(e, HostEvent::Ended(_))).await;
    sleep(Duration::from_millis(300)).await;
    h.shutdown();

    let mut logs = Vec::new();
    for t in tasks.into_iter().chain(late) {
        logs.push(
            timeout(T, t)
                .await
                .expect("client stuck after shutdown")
                .unwrap(),
        );
    }
    ev_until_closed(&mut h).await;
    for l in &logs {
        check_log(l);
        assert!(
            l.disconnected,
            "{} never saw the end of the connection",
            l.name
        );
    }
    let steady: Vec<&Log> = logs.iter().filter(|l| l.name.starts_with('S')).collect();
    for l in &steady {
        assert_eq!(l.ends.len(), 2, "{}: expected End for both games", l.name);
        assert_eq!(l.reveals.len(), 4 + 3, "{}: missing reveals", l.name);
        assert_eq!(
            l.reveals, steady[0].reveals,
            "{} sees different scores",
            l.name
        );
    }
    // Game 2 Q1: S0 muted, so it gained nothing there.
    let (_, _, g2q1) = steady[0]
        .reveals
        .iter()
        .find(|(g, q, _)| *g == 1 && *q == 0)
        .unwrap();
    assert_eq!(g2q1.iter().find(|e| e.name == "S0").unwrap().choice, None);
    // Late joiners ended game 1 and saw game 2 too.
    for l in logs.iter().filter(|l| l.name.starts_with("Late")) {
        assert_eq!(l.ends.len(), 2, "{}", l.name);
    }
}

async fn ev_until_closed(h: &mut HostHandle) {
    loop {
        match timeout(T, h.events.recv()).await {
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(_) => return, // actor already gone; senders kept alive by nothing else
        }
    }
}

async fn retry_join(ip: IpAddr, port: u16, name: &str) -> ClientHandle {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match connect(ip, port, name).await {
            Ok(c) => return c,
            Err(NetError::Rejected(ErrorCode::LobbyFull)) if Instant::now() < end => {
                sleep(Duration::from_millis(50)).await
            }
            Err(e) => panic!("{name}: {e}"),
        }
    }
}

/// Unlimited game: the only missing answer comes from a client that is reset
/// (RST): the host must reveal by itself, no force_reveal needed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abrupt_reset_unblocks_unlimited_question() {
    let ip = target_ip();
    let mut h = Host::start(HostConfig {
        port: 0,
        announce: false,
        time_limit: None,
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let mut a = connect(ip, h.port(), "A").await.unwrap();
    spawn_crasher(ip, h.port(), "Doomed").await;
    h.start_game(questions(1), None);
    loop {
        if let ClientEvent::Message(ServerMsg::Question(_)) =
            timeout(T, a.events.recv()).await.unwrap().unwrap()
        {
            break;
        }
    }
    a.answer(0, 2);
    h.host_answer(2);
    let HostEvent::Revealed(info) = ev_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await
    else {
        unreachable!()
    };
    assert_eq!(info.scores.len(), 3);
}

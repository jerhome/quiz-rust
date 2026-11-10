use quiz_core::Question;
use quiz_net::{
    discover_on, Client, ClientEvent, ClientHandle, ErrorCode, GamePhase, Host, HostConfig,
    HostEvent, HostHandle, NetError, ServerMsg, PROTOCOL_VERSION,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

const T: Duration = Duration::from_secs(3);

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

async fn host(time_limit: Option<Duration>) -> HostHandle {
    Host::start(HostConfig {
        name: "Hôte".into(),
        port: 0,
        time_limit,
        announce: false,
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap()
}

async fn join(h: &HostHandle, name: &str) -> Result<ClientHandle, NetError> {
    Client::connect(("127.0.0.1", h.port()), name, None).await
}

async fn host_event_until<F: Fn(&HostEvent) -> bool>(h: &mut HostHandle, f: F) -> HostEvent {
    loop {
        let ev = timeout(T, h.events.recv()).await.unwrap().unwrap();
        if f(&ev) {
            return ev;
        }
    }
}

async fn wait_progress(h: &mut HostHandle, n: usize) {
    host_event_until(
        h,
        |e| matches!(e, HostEvent::AnswerProgress { answered, .. } if *answered == n),
    )
    .await;
}

async fn client_msg_until<F: Fn(&ServerMsg) -> bool>(c: &mut ClientHandle, f: F) -> ServerMsg {
    loop {
        match timeout(T, c.events.recv()).await.unwrap().unwrap() {
            ClientEvent::Message(m) if f(&m) => return m,
            ClientEvent::Disconnected => panic!("déconnecté"),
            _ => {}
        }
    }
}

fn is_question(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::Question(_))
}
fn is_reveal(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::Reveal { .. })
}

#[tokio::test]
async fn full_game_with_two_clients() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c2 = counter.clone();
    let mut h = Host::start(HostConfig {
        port: 0,
        announce: false,
        time_limit: None,
        waker: Some(Arc::new(move || {
            c2.fetch_add(1, Ordering::SeqCst);
        })),
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let mut alice = join(&h, "Alice").await.unwrap();
    let mut bob = join(&h, "Bob").await.unwrap();
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if p.len() == 3),
    )
    .await;
    let ServerMsg::Lobby { players, .. } = client_msg_until(
        &mut bob,
        |m| matches!(m, ServerMsg::Lobby { players, .. } if players.len() == 3),
    )
    .await
    else {
        unreachable!()
    };
    assert!(players[0].is_host);

    assert!(matches!(
        join(&h, "alice").await,
        Err(NetError::Rejected(ErrorCode::NameTaken))
    ));

    h.start_game(questions(2), None);
    for c in [&mut alice, &mut bob] {
        let ServerMsg::Question(q) = client_msg_until(c, is_question).await else {
            unreachable!()
        };
        assert_eq!((q.id, q.index, q.total), (0, 0, 2));
        assert_eq!((q.time_limit_secs, q.remaining_ms), (None, None));
    }
    alice.answer(0, 2);
    // Answers are sent one at a time, waiting for host progress, so the order-based
    // bonus is deterministic.
    wait_progress(&mut h, 1).await;
    bob.answer(0, 0);
    bob.answer(0, 2);
    wait_progress(&mut h, 2).await;
    h.host_answer(2);
    let ServerMsg::Reveal {
        correct, scores, ..
    } = client_msg_until(&mut alice, is_reveal).await
    else {
        unreachable!()
    };
    assert_eq!(correct, 2);
    let s: Vec<(u32, u32, Option<usize>)> = scores
        .iter()
        .map(|e| (e.score, e.gained, e.choice))
        .collect();
    assert_eq!(
        s,
        [(900, 900, Some(2)), (1000, 1000, Some(2)), (0, 0, Some(0))]
    );

    h.next_question();
    client_msg_until(&mut bob, is_question).await;
    alice.answer(1, 2);
    wait_progress(&mut h, 1).await;
    bob.answer(1, 2);
    wait_progress(&mut h, 2).await;
    h.host_answer(1);
    client_msg_until(&mut bob, is_reveal).await;
    h.next_question();
    let ServerMsg::End { ranking } =
        client_msg_until(&mut bob, |m| matches!(m, ServerMsg::End { .. })).await
    else {
        unreachable!()
    };
    let r: Vec<(u32, &str, u32)> = ranking
        .iter()
        .map(|e| (e.rank, e.name.as_str(), e.score))
        .collect();
    assert_eq!(r, [(1, "Alice", 2000), (2, "Bob", 900), (2, "Hôte", 900)]);
    host_event_until(&mut h, |e| matches!(e, HostEvent::Ended(_))).await;
    assert!(counter.load(Ordering::SeqCst) > 0, "waker appelé");
}

/// Raw client used to inspect exactly what the host puts on the wire.
async fn raw_join(port: u16, name: &str, version: u32) -> BufReader<TcpStream> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let join =
        format!("{{\"type\":\"Join\",\"name\":\"{name}\",\"protocol_version\":{version}}}\n");
    s.write_all(join.as_bytes()).await.unwrap();
    BufReader::new(s)
}

async fn raw_line(r: &mut BufReader<TcpStream>) -> Option<String> {
    let mut l = String::new();
    let n = timeout(T, r.read_line(&mut l)).await.unwrap().unwrap_or(0);
    (n > 0).then_some(l)
}

#[tokio::test]
async fn wrong_version_is_refused_then_closed() {
    let h = host(Some(Duration::from_secs(5))).await;
    let mut r = raw_join(h.port(), "Zed", PROTOCOL_VERSION + 1).await;
    let l = raw_line(&mut r).await.unwrap();
    assert!(
        l.contains("\"Error\"") && l.contains("\"VersionMismatch\""),
        "{l}"
    );
    assert!(raw_line(&mut r).await.is_none(), "connexion fermée");
}

#[tokio::test]
async fn join_during_question_gets_remaining_time_and_can_answer() {
    let mut h = host(Some(Duration::from_secs(5))).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(1), Some(Duration::from_secs(5)));
    client_msg_until(&mut alice, is_question).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut late = join(&h, "Late").await.unwrap();
    let ServerMsg::Question(q) = client_msg_until(&mut late, is_question).await else {
        unreachable!()
    };
    assert_eq!(q.time_limit_secs, Some(5));
    let rem = q.remaining_ms.unwrap();
    assert!((1..=4_900).contains(&rem), "remaining_ms = {rem}");
    late.answer(0, 2);
    wait_progress(&mut h, 1).await;
    alice.answer(0, 2);
    h.host_answer(2);
    let ServerMsg::Reveal { scores, .. } = client_msg_until(&mut late, is_reveal).await else {
        unreachable!()
    };
    assert!(scores.iter().all(|s| s.gained >= 500));
}

#[tokio::test]
async fn join_during_reveal_waits_for_next_question() {
    let mut h = host(None).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(2), None);
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    h.host_answer(2);
    client_msg_until(&mut alice, is_reveal).await;
    let mut late = join(&h, "Late").await.unwrap();
    client_msg_until(
        &mut late,
        |m| matches!(m, ServerMsg::Lobby { players, .. } if players.len() == 3),
    )
    .await;
    h.next_question();
    let ServerMsg::Question(q) =
        client_msg_until(&mut late, |m| is_question(m) || is_reveal(m)).await
    else {
        panic!("la question suivante était attendue, pas une révélation");
    };
    assert_eq!(q.index, 1);
    assert_eq!(q.remaining_ms, None);
    host_event_until(&mut h, |e| {
        matches!(e, HostEvent::AnswerProgress { expected: 3, .. })
    })
    .await;
}

#[tokio::test]
async fn back_to_lobby_then_second_game_resets_scores() {
    let mut h = host(None).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.back_to_lobby();
    host_event_until(&mut h, |e| matches!(e, HostEvent::Error(_))).await;

    h.start_game(questions(1), None);
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    h.host_answer(2);
    client_msg_until(&mut alice, is_reveal).await;
    h.next_question();
    client_msg_until(&mut alice, |m| matches!(m, ServerMsg::End { .. })).await;

    let mut carol = join(&h, "Carol").await.unwrap();
    let ServerMsg::End { ranking } =
        client_msg_until(&mut carol, |m| matches!(m, ServerMsg::End { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(ranking.iter().find(|r| r.name == "Carol").unwrap().score, 0);

    h.back_to_lobby();
    host_event_until(&mut h, |e| matches!(e, HostEvent::BackToLobby)).await;
    for c in [&mut alice, &mut carol] {
        client_msg_until(c, |m| matches!(m, ServerMsg::BackToLobby)).await;
    }
    h.start_game(questions(1), None);
    client_msg_until(&mut carol, is_question).await;
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    wait_progress(&mut h, 1).await;
    h.host_answer(1);
    carol.answer(0, 2);
    let ServerMsg::Reveal { scores, .. } = client_msg_until(&mut carol, is_reveal).await else {
        unreachable!()
    };
    assert_eq!(scores.len(), 3, "joueurs conservés");
    for s in &scores {
        assert_eq!(s.score, s.gained, "scores remis à zéro : {s:?}");
    }
    assert_eq!(
        scores.iter().find(|s| s.name == "Alice").unwrap().gained,
        1000
    );
    assert_eq!(
        scores.iter().find(|s| s.name == "Carol").unwrap().gained,
        900
    );
}

#[tokio::test]
async fn unlimited_has_no_timeout_and_force_reveal_works() {
    let mut h = host(None).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(1), None);
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    tokio::time::sleep(Duration::from_millis(400)).await;
    while let Ok(ev) = h.events.try_recv() {
        assert!(
            !matches!(ev, HostEvent::Revealed(_)),
            "pas de timeout en illimité"
        );
    }
    h.force_reveal();
    let HostEvent::Revealed(info) =
        host_event_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await
    else {
        unreachable!()
    };
    assert_eq!(
        info.scores.iter().map(|s| s.gained).collect::<Vec<_>>(),
        [0, 1000]
    );
    client_msg_until(&mut alice, is_reveal).await;
}

#[tokio::test]
async fn two_discovery_listeners_share_the_port() {
    let port = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    };
    let mut d1 = discover_on(port, None).await.unwrap();
    let mut d2 = discover_on(port, None).await.unwrap();
    let u = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    u.send_to(
        br#"{"game":"quiz-rust","name":"X","port":7878}"#,
        ("127.0.0.1", port),
    )
    .await
    .unwrap();
    // Unicast reaches only one socket; the test proves both binds succeed.
    let got = timeout(T, async {
        tokio::select! { h = d1.hosts.recv() => h, h = d2.hosts.recv() => h }
    })
    .await
    .unwrap();
    assert_eq!(got.unwrap().name, "X");
}

#[tokio::test]
async fn nothing_secret_before_reveal_on_the_wire() {
    let h = host(Some(Duration::from_secs(5))).await;
    let mut raw = raw_join(h.port(), "Raw", PROTOCOL_VERSION).await;
    let alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(1), None);
    let mut lines = vec![];
    loop {
        let l = raw_line(&mut raw).await.unwrap();
        let is_q = l.contains("\"Question\"");
        lines.push(l);
        if is_q {
            break;
        }
    }
    for l in &lines {
        assert!(
            !l.contains("EXPLICATION_SECRETE") && !l.contains("explanation"),
            "{l}"
        );
        assert!(!l.contains("correct"), "{l}");
    }
    alice.answer(0, 1);
    h.host_answer(1);
    let mut raw_w = raw.into_inner();
    raw_w
        .write_all(b"{\"type\":\"Answer\",\"question_id\":0,\"choice\":3}\n")
        .await
        .unwrap();
    let mut raw = BufReader::new(raw_w);
    loop {
        let l = raw_line(&mut raw).await.unwrap();
        if l.contains("\"Reveal\"") {
            assert!(l.contains("EXPLICATION_SECRETE"));
            break;
        }
    }
}

#[tokio::test]
async fn reveal_on_timeout() {
    let mut h = host(Some(Duration::from_millis(300))).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(1), Some(Duration::from_millis(300)));
    alice.answer(0, 2);
    let ServerMsg::Reveal { scores, .. } = client_msg_until(&mut alice, is_reveal).await else {
        unreachable!()
    };
    assert_eq!(scores[0].score, 0);
    assert!((900..=1000).contains(&scores[1].score), "{scores:?}");
    host_event_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
    h.host_answer(2);
}

#[tokio::test]
async fn disconnect_in_lobby_and_in_game() {
    let mut h = host(Some(Duration::from_secs(5))).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    let mut bob = join(&h, "Bob").await.unwrap();
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if p.len() == 3),
    )
    .await;
    bob.disconnect();
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if p.iter().filter(|x| x.connected).count() == 2),
    )
    .await;
    h.start_game(questions(1), None);
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    drop(alice);
    h.host_answer(2);
    host_event_until(&mut h, |e| matches!(e, HostEvent::Revealed(_))).await;
}

#[tokio::test]
async fn oversized_line_disconnects_cleanly() {
    let h = host(Some(Duration::from_secs(5))).await;
    let mut raw = raw_join(h.port(), "Big", PROTOCOL_VERSION).await;
    assert!(raw_line(&mut raw).await.unwrap().contains("Welcome"));
    let mut s = raw.into_inner();
    let _ = s.write_all(&vec![b'x'; 70 * 1024]).await;
    let mut r = BufReader::new(s);
    loop {
        if raw_line(&mut r).await.is_none() {
            break;
        }
    }
    join(&h, "Ok").await.unwrap();
}

#[tokio::test]
async fn invalid_json_disconnects_cleanly() {
    let h = host(Some(Duration::from_secs(5))).await;
    let mut raw = raw_join(h.port(), "Bad", PROTOCOL_VERSION).await;
    assert!(raw_line(&mut raw).await.unwrap().contains("Welcome"));
    let mut s = raw.into_inner();
    s.write_all(b"pas du json\n").await.unwrap();
    let mut r = BufReader::new(s);
    while raw_line(&mut r).await.is_some() {}
    join(&h, "Ok").await.unwrap();
}

#[tokio::test]
async fn shutdown_disconnects_clients() {
    let h = host(Some(Duration::from_secs(5))).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.shutdown();
    loop {
        if let ClientEvent::Disconnected = timeout(T, alice.events.recv()).await.unwrap().unwrap() {
            break;
        }
    }
}

#[tokio::test]
async fn host_announces_and_client_discovers() {
    let port = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    };
    let mut d = discover_on(port, None).await.unwrap();
    let u = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    u.send_to(b"garbage", ("127.0.0.1", port)).await.unwrap();
    let h = Host::start(HostConfig {
        name: "Salon".into(),
        port: 0,
        announce: true,
        discovery_port: port,
        announce_ip: "127.0.0.1".parse().unwrap(),
        web_port: None,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let found = timeout(T, d.hosts.recv()).await.unwrap().unwrap();
    assert_eq!(found.name, "Salon");
    assert_eq!(found.addr.port(), h.port());
    Client::connect(found.addr, "Zoe", None).await.unwrap();
}

#[tokio::test]
async fn join_during_reveal_reports_reveal_phase() {
    let h = host(None).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    h.start_game(questions(2), None);
    client_msg_until(&mut alice, is_question).await;
    alice.answer(0, 2);
    h.host_answer(2);
    client_msg_until(&mut alice, is_reveal).await;
    let mut late = join(&h, "Late").await.unwrap();
    let ServerMsg::Lobby { phase, .. } =
        client_msg_until(&mut late, |m| matches!(m, ServerMsg::Lobby { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(phase, GamePhase::Reveal);
    let h2 = host(None).await;
    let mut c = join(&h2, "Zoe").await.unwrap();
    let ServerMsg::Lobby { phase, .. } =
        client_msg_until(&mut c, |m| matches!(m, ServerMsg::Lobby { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(phase, GamePhase::Lobby);
    h.next_question();
}

#[tokio::test]
async fn time_limit_can_change_between_games() {
    let mut h = host(None).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    let question = |limit: Option<Duration>| {
        h.start_game(questions(1), limit);
    };
    question(None);
    let ServerMsg::Question(q) = client_msg_until(&mut alice, is_question).await else {
        unreachable!()
    };
    assert_eq!((q.time_limit_secs, q.remaining_ms), (None, None));
    alice.answer(0, 2);
    h.host_answer(2);
    client_msg_until(&mut alice, is_reveal).await;
    h.next_question();
    client_msg_until(&mut alice, |m| matches!(m, ServerMsg::End { .. })).await;
    h.back_to_lobby();
    client_msg_until(&mut alice, |m| matches!(m, ServerMsg::BackToLobby)).await;

    h.start_game(questions(1), Some(Duration::from_secs(3)));
    let ServerMsg::Question(q) = client_msg_until(&mut alice, is_question).await else {
        unreachable!()
    };
    assert_eq!(q.time_limit_secs, Some(3));
    assert!(q.remaining_ms.unwrap() <= 3000);
    alice.answer(0, 2);
    h.host_answer(2);
    client_msg_until(&mut alice, is_reveal).await;
    h.next_question();
    client_msg_until(&mut alice, |m| matches!(m, ServerMsg::End { .. })).await;
    h.back_to_lobby();
    client_msg_until(&mut alice, |m| matches!(m, ServerMsg::BackToLobby)).await;

    h.start_game(questions(1), Some(Duration::from_secs(8)));
    let ServerMsg::Question(q) = client_msg_until(&mut alice, is_question).await else {
        unreachable!()
    };
    assert_eq!(q.time_limit_secs, Some(8));
    h.start_game(questions(1), None);
    host_event_until(&mut h, |e| matches!(e, HostEvent::Error(_))).await;
}

#[tokio::test]
async fn limited_game_speed_bonus_then_second_game() {
    let limit = Duration::from_secs(4);
    let mut h = host(Some(limit)).await;
    let mut alice = join(&h, "Alice").await.unwrap();
    let mut bob = join(&h, "Bob").await.unwrap();
    host_event_until(
        &mut h,
        |e| matches!(e, HostEvent::LobbyChanged(p) if p.len() == 3),
    )
    .await;

    for game in 0..2 {
        h.start_game(questions(2), Some(limit));
        for q in 0..2u32 {
            for c in [&mut alice, &mut bob] {
                let ServerMsg::Question(p) = client_msg_until(c, is_question).await else {
                    unreachable!()
                };
                assert_eq!((p.id, p.time_limit_secs), (q, Some(4)));
            }
            alice.answer(q, 2);
            wait_progress(&mut h, 1).await;
            tokio::time::sleep(Duration::from_millis(600)).await;
            bob.answer(q, 2);
            h.host_answer(1);
            let ServerMsg::Reveal { scores, .. } = client_msg_until(&mut bob, is_reveal).await
            else {
                unreachable!()
            };
            let by = |n: &str| scores.iter().find(|s| s.name == n).unwrap().clone();
            let (a, b, host) = (by("Alice"), by("Bob"), by("Hôte"));
            assert!((500..=1000).contains(&b.gained), "{b:?}");
            assert!(a.gained > b.gained, "bonus de vitesse : {a:?} vs {b:?}");
            assert!(a.gained >= 950, "{a:?}");
            assert_eq!(host.gained, 0);
            let expected = |g: u32| g * (q + 1);
            assert!(
                a.score >= 950 * (q + 1) && a.score <= expected(1000),
                "{a:?}"
            );
            assert_eq!(host.score, 0, "partie {game}");
            h.next_question();
        }
        let ServerMsg::End { ranking } =
            client_msg_until(&mut bob, |m| matches!(m, ServerMsg::End { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(ranking[0].name, "Alice");
        assert_eq!(ranking[1].name, "Bob");
        assert_eq!(
            (ranking[0].rank, ranking[1].rank, ranking[2].rank),
            (1, 2, 3)
        );
        assert!(ranking[0].score <= 2000 && ranking[2].score == 0);
        client_msg_until(&mut alice, |m| matches!(m, ServerMsg::End { .. })).await;
        h.back_to_lobby();
        for c in [&mut alice, &mut bob] {
            client_msg_until(c, |m| matches!(m, ServerMsg::BackToLobby)).await;
        }
    }
}

#[tokio::test]
async fn refusal_codes_reach_the_client() {
    let h = host(None).await;
    let _a = join(&h, "Alice").await.unwrap();
    assert!(matches!(
        join(&h, "  ").await,
        Err(NetError::Rejected(ErrorCode::NameEmpty))
    ));
    assert!(matches!(
        join(&h, &"x".repeat(30)).await,
        Err(NetError::Rejected(ErrorCode::NameTooLong))
    ));
    let e = join(&h, "alice").await.err().unwrap();
    assert!(e.message(quiz_core::Lang::En).contains("already taken"));
    assert!(e.message(quiz_core::Lang::Fr).contains("déjà pris"));
}

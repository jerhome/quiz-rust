//! Realistic LAN conditions: non-loopback IP, real UDP broadcast (255.255.255.255) on a
//! dedicated discovery port. Skips (passes) when the machine has no routable IPv4.
use quiz_core::Question;
use quiz_net::{
    discover_on, Client, ClientEvent, ClientHandle, Host, HostConfig, HostEvent, ServerMsg,
};
use std::net::{IpAddr, UdpSocket};
use std::time::Duration;
use tokio::time::timeout;

const T: Duration = Duration::from_secs(8);

/// Same trick as the app: a connected UDP socket only selects the outgoing route.
fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn questions(n: u32) -> Vec<Question> {
    (0..n)
        .map(|id| Question {
            id,
            prompt: format!("Q{id}"),
            choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct: 1,
            explanation: "e".into(),
        })
        .collect()
}

async fn until_msg<F: Fn(&ServerMsg) -> bool>(c: &mut ClientHandle, f: F) -> ServerMsg {
    loop {
        match timeout(T, c.events.recv()).await.unwrap().unwrap() {
            ClientEvent::Message(m) if f(&m) => return m,
            ClientEvent::Disconnected => panic!("disconnected"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn discovery_by_real_broadcast_then_play_on_lan_ip() {
    let Some(ip) = lan_ip() else {
        eprintln!("SKIP: no non-loopback IPv4");
        return;
    };
    let dport = UdpSocket::bind("0.0.0.0:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut disc = discover_on(dport, None).await.unwrap();
    let mut h = Host::start(HostConfig {
        name: "Salle".into(),
        port: 0,
        time_limit: None,
        announce: true,
        discovery_port: dport,
        web_port: None,
        ..HostConfig::default() // announce_ip = 255.255.255.255, as in production
    })
    .await
    .unwrap();

    let found = timeout(Duration::from_secs(6), disc.hosts.recv())
        .await
        .expect("no broadcast announcement received within 6 s")
        .unwrap();
    assert_eq!(found.name, "Salle");
    assert_eq!(found.addr.port(), h.port());
    assert!(
        !found.addr.ip().is_loopback(),
        "announced address is loopback: {}",
        found.addr
    );
    assert_eq!(found.addr.ip(), ip, "announced IP differs from route IP");

    // Connect through the announced address and through the LAN IP explicitly.
    let mut a = Client::connect(found.addr, "Alice", None).await.unwrap();
    let mut b = Client::connect((ip, h.port()), "Bob", None).await.unwrap();
    h.start_game(questions(2), None);
    for c in [&mut a, &mut b] {
        until_msg(c, |m| matches!(m, ServerMsg::Question(_))).await;
    }
    a.answer(0, 1);
    b.answer(0, 1);
    h.host_answer(1);
    for c in [&mut a, &mut b] {
        until_msg(c, |m| matches!(m, ServerMsg::Reveal { .. })).await;
    }
    h.next_question();
    for c in [&mut a, &mut b] {
        until_msg(c, |m| matches!(m, ServerMsg::Question(_))).await;
    }
    a.answer(1, 1);
    b.answer(1, 0);
    h.host_answer(1);
    until_msg(&mut a, |m| matches!(m, ServerMsg::Reveal { .. })).await;
    h.next_question();
    let ServerMsg::End { ranking } =
        until_msg(&mut a, |m| matches!(m, ServerMsg::End { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(ranking.len(), 3);
    loop {
        let ev = timeout(T, h.events.recv()).await.unwrap().unwrap();
        if matches!(ev, HostEvent::Ended(_)) {
            break;
        }
    }
}

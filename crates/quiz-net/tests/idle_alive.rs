//! A quiet lobby / long idle period must not trip the heartbeat: pings flow even when
//! nothing else is sent and the client answers from its own task.
use quiz_net::{Client, ClientEvent, HeartbeatConfig, Host, HostConfig, ServerMsg};
use std::time::Duration;
use tokio::time::timeout;

const HB: HeartbeatConfig = HeartbeatConfig {
    interval: Duration::from_millis(100),
    idle_timeout: Duration::from_millis(500),
};

#[tokio::test]
async fn idle_lobby_outlives_many_idle_timeouts() {
    let h = Host::start(HostConfig {
        port: 0,
        announce: false,
        time_limit: None,
        heartbeat: HB,
        ..HostConfig::default()
    })
    .await
    .unwrap();
    let mut c = Client::connect_with(("127.0.0.1", h.port()), "Alice", None, HB)
        .await
        .unwrap();
    // Several idle_timeouts of total silence at application level, UI never touches the client.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let mut got_disconnect = false;
    while let Ok(ev) = c.events.try_recv() {
        if matches!(ev, ClientEvent::Disconnected) {
            got_disconnect = true;
        }
    }
    assert!(!got_disconnect, "client dropped during an idle lobby");
    // Still registered on the host: a lobby update reaches the client.
    let _bob = Client::connect_with(("127.0.0.1", h.port()), "Bob", None, HB)
        .await
        .unwrap();
    let saw_lobby = timeout(Duration::from_secs(2), async {
        loop {
            match c.events.recv().await {
                Some(ClientEvent::Message(ServerMsg::Lobby { players, .. }))
                    if players.iter().any(|p| p.name == "Bob") =>
                {
                    return true
                }
                Some(ClientEvent::Disconnected) | None => return false,
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(saw_lobby, Ok(true));
}

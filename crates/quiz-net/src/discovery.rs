use crate::error::NetError;
use crate::protocol::DISCOVERY_PORT;
use crate::Waker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::JoinHandle;

const GAME_ID: &str = "quiz-rust";
pub(crate) const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Serialize, Deserialize)]
struct Announcement {
    game: String,
    name: String,
    port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub name: String,
    pub addr: SocketAddr,
}

pub struct DiscoveryHandle {
    pub hosts: UnboundedReceiver<DiscoveredHost>,
    task: JoinHandle<()>,
}

impl Drop for DiscoveryHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn parse_announcement(bytes: &[u8]) -> Option<Announcement> {
    let a: Announcement = serde_json::from_slice(bytes).ok()?;
    (a.game == GAME_ID && a.port != 0 && !a.name.trim().is_empty()).then_some(a)
}

pub async fn discover(waker: Option<Waker>) -> Result<DiscoveryHandle, NetError> {
    discover_on(DISCOVERY_PORT, waker).await
}

pub async fn discover_on(port: u16, waker: Option<Waker>) -> Result<DiscoveryHandle, NetError> {
    let socket = bind_shared(port)?;
    let (tx, rx) = unbounded_channel();
    let task = tokio::spawn(async move {
        let mut seen: HashMap<SocketAddr, String> = HashMap::new();
        let mut buf = [0u8; 1024];
        loop {
            let Ok((n, from)) = socket.recv_from(&mut buf).await else {
                continue;
            };
            let Some(a) = parse_announcement(&buf[..n]) else {
                continue;
            };
            let addr = SocketAddr::new(from.ip(), a.port);
            // Hosts re-announce every 2 s: only report new hosts or renamed ones.
            if seen.get(&addr) == Some(&a.name) {
                continue;
            }
            seen.insert(addr, a.name.clone());
            if tx.send(DiscoveredHost { name: a.name, addr }).is_err() {
                break;
            }
            if let Some(w) = &waker {
                w();
            }
        }
    });
    Ok(DiscoveryHandle { hosts: rx, task })
}

/// SO_REUSEADDR + SO_REUSEPORT let several instances on one machine listen on the same
/// port (handy for local testing); broadcasts are delivered to each of them.
fn bind_shared(port: u16) -> Result<UdpSocket, NetError> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from(([0, 0, 0, 0], port)).into())?;
    Ok(UdpSocket::from_std(socket.into())?)
}

pub(crate) async fn announce_loop(name: String, tcp_port: u16, target: SocketAddr) {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
        tracing::warn!("annonce UDP : bind impossible");
        return;
    };
    if socket.set_broadcast(true).is_err() {
        tracing::warn!("annonce UDP : broadcast indisponible");
    }
    let Ok(payload) = serde_json::to_vec(&Announcement {
        game: GAME_ID.into(),
        name,
        port: tcp_port,
    }) else {
        return;
    };
    let mut tick = tokio::time::interval(ANNOUNCE_INTERVAL);
    loop {
        tick.tick().await;
        if let Err(e) = socket.send_to(&payload, target).await {
            tracing::debug!("annonce UDP échouée : {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_valid_announcements() {
        assert!(
            parse_announcement(br#"{"game":"quiz-rust","name":"Salon","port":7878}"#).is_some()
        );
        assert!(parse_announcement(br#"{"game":"autre","name":"x","port":1}"#).is_none());
        assert!(parse_announcement(br#"{"game":"quiz-rust","name":"x","port":0}"#).is_none());
        assert!(parse_announcement(br#"{"game":"quiz-rust","name":" ","port":5}"#).is_none());
        assert!(parse_announcement(b"garbage").is_none());
        assert!(parse_announcement(br#"{"game":"quiz-rust"}"#).is_none());
    }
}

use crate::error::NetError;
use crate::netif::{announce_targets, system_ifaces};
use crate::protocol::DISCOVERY_PORT;
use crate::Waker;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::JoinHandle;

const GAME_ID: &str = "quiz-rust";
const RECV_ERROR_PAUSE: Duration = Duration::from_millis(200);
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
    /// When the latest announcement from this host arrived. Hosts are reported again
    /// on every announcement, so a UI can expire entries whose `last_seen` gets old.
    pub last_seen: Instant,
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

/// Errors that can never succeed on retry. Windows reports ICMP "port unreachable"
/// from earlier sends as `ConnectionReset` on UDP receives, which is harmless.
fn is_fatal(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    !matches!(
        e.kind(),
        ConnectionReset
            | ConnectionRefused
            | ConnectionAborted
            | Interrupted
            | WouldBlock
            | TimedOut
    )
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
        let mut buf = [0u8; 1024];
        loop {
            let (n, from) = match socket.recv_from(&mut buf).await {
                Ok(x) => x,
                Err(e) if is_fatal(&e) => {
                    tracing::warn!("discovery stopped: {e}");
                    break;
                }
                Err(e) => {
                    // Without the pause a persistent error would spin a core at 100 %.
                    tracing::debug!("discovery recv error: {e}");
                    tokio::time::sleep(RECV_ERROR_PAUSE).await;
                    continue;
                }
            };
            let Some(a) = parse_announcement(&buf[..n]) else {
                continue;
            };
            let host = DiscoveredHost {
                name: a.name,
                addr: SocketAddr::new(from.ip(), a.port),
                last_seen: Instant::now(),
            };
            if tx.send(host).is_err() {
                break;
            }
            if let Some(w) = &waker {
                w();
            }
        }
    });
    Ok(DiscoveryHandle { hosts: rx, task })
}

/// Lets several instances on one machine listen on the same port (handy for local
/// testing); broadcasts are delivered to each of them. On Unix this needs SO_REUSEPORT
/// as well; on Windows SO_REUSEADDR alone already has that meaning for UDP and
/// `set_reuse_port` does not exist.
fn bind_shared(port: u16) -> Result<UdpSocket, NetError> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from(([0, 0, 0, 0], port)).into())?;
    Ok(UdpSocket::from_std(socket.into())?)
}

pub(crate) enum AnnounceTarget {
    /// Directed broadcast on every interface plus 255.255.255.255, recomputed each round
    /// because interfaces come and go (VPN, Wi-Fi roaming).
    Lan(u16),
    Fixed(SocketAddr),
}

pub(crate) async fn announce_loop(name: String, tcp_port: u16, target: AnnounceTarget) {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
        tracing::warn!("UDP announce: bind failed");
        return;
    };
    if socket.set_broadcast(true).is_err() {
        tracing::warn!("UDP announce: broadcast unavailable");
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
        let targets = match &target {
            AnnounceTarget::Lan(port) => announce_targets(&system_ifaces(), *port),
            AnnounceTarget::Fixed(a) => vec![*a],
        };
        for t in targets {
            if let Err(e) = socket.send_to(&payload, t).await {
                tracing::debug!("UDP announce to {t} failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_errors_are_not_fatal() {
        use std::io::{Error, ErrorKind};
        assert!(!is_fatal(&Error::from(ErrorKind::ConnectionReset)));
        assert!(!is_fatal(&Error::from(ErrorKind::Interrupted)));
        assert!(is_fatal(&Error::from(ErrorKind::InvalidInput)));
        assert!(is_fatal(&Error::from(ErrorKind::PermissionDenied)));
    }

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

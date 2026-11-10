//! LAN multiplayer: TCP host/client (JSON lines), UDP discovery, pure game logic.
//! Everything is non-blocking; callers drain `events` receivers with `try_recv()`.

mod client;
mod conn;
mod discovery;
mod error;
pub mod game;
mod heartbeat;
mod io;
mod netif;
mod protocol;
mod server;
mod web;

use std::sync::Arc;

/// Called on every event so the UI can `request_repaint()` without depending on egui.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

pub use client::{Client, ClientEvent, ClientHandle};
pub use discovery::{discover, discover_on, DiscoveredHost, DiscoveryHandle};
pub use error::NetError;
pub use game::{JoinError, PlayerId, HOST_ID, MAX_PLAYERS};
pub use heartbeat::HeartbeatConfig;
pub use netif::{local_ipv4_addrs, LocalAddr};
pub use protocol::{
    ClientMsg, Device, ErrorCode, GamePhase, PlayerInfo, PublicQuestion, RankEntry, ScoreEntry,
    ServerMsg, DEFAULT_PORT, DISCOVERY_PORT, MAX_LINE_BYTES, MAX_WS_FRAME_BYTES, PROTOCOL_VERSION,
};
pub use server::{Host, HostConfig, HostEvent, HostHandle, DEFAULT_WEB_PORT};
pub use web::WebAssets;

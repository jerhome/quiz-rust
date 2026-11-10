//! Transport-independent connection task: TCP and WebSocket both feed it text messages
//! (one JSON document each) and drain `ServerMsg`s, so the handshake and heartbeat
//! rules live in one place.

use crate::game::PlayerId;
use crate::protocol::{ClientBeat, ClientMsg, Device, ErrorCode, ServerMsg};
use crate::server::{error_msg, Cmd};
use crate::HeartbeatConfig;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, UnboundedSender};
use tokio::sync::{oneshot, watch};

// A connection that does not send `Join` in time is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Link {
    /// Closing it means the peer is gone (EOF, error, oversized or invalid frame).
    pub inbound: Receiver<String>,
    pub out: UnboundedSender<ServerMsg>,
    pub device: Device,
}

pub(crate) async fn serve(
    link: Link,
    cmd: UnboundedSender<Cmd>,
    mut stop: watch::Receiver<bool>,
    hb: HeartbeatConfig,
) {
    let Link {
        mut inbound,
        out,
        device,
    } = link;
    let conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
    let (kill, mut killed) = oneshot::channel::<()>();

    let joined = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        handshake(&mut inbound, &out, &cmd, device, conn_id, kill),
    )
    .await;
    // Rejected: the actor already queued the error; dropping `out` lets the writer flush it and close.
    let Ok(Some(id)) = joined else { return };

    loop {
        let msg = tokio::select! {
            _ = stop.changed() => break,
            _ = &mut killed => break,
            // Anything received (pongs included) proves the player is alive.
            m = tokio::time::timeout(hb.idle_timeout, inbound.recv()) => m,
        };
        let Ok(Some(text)) = msg else { break };
        match serde_json::from_str::<ClientMsg>(&text) {
            Ok(ClientMsg::Answer {
                question_id,
                choice,
            }) => {
                let _ = cmd.send(Cmd::Answer {
                    id,
                    question_id,
                    choice,
                });
            }
            Err(_) if serde_json::from_str::<ClientBeat>(&text).is_ok() => {}
            Ok(ClientMsg::Join { .. }) | Err(_) => {
                let _ = out.send(error_msg(ErrorCode::InvalidMessage));
                break;
            }
        }
    }
    let _ = cmd.send(Cmd::Disconnected { id, conn_id });
}

async fn handshake(
    inbound: &mut Receiver<String>,
    out: &UnboundedSender<ServerMsg>,
    cmd: &UnboundedSender<Cmd>,
    device: Device,
    conn_id: u64,
    kill: oneshot::Sender<()>,
) -> Option<PlayerId> {
    let text = inbound.recv().await?;
    let Ok(ClientMsg::Join {
        name,
        protocol_version,
        resume,
    }) = serde_json::from_str::<ClientMsg>(&text)
    else {
        let _ = out.send(error_msg(ErrorCode::ExpectedJoin));
        return None;
    };
    let (reply, rx) = oneshot::channel();
    cmd.send(Cmd::Join {
        name,
        version: protocol_version,
        resume,
        device,
        conn_id,
        out: out.clone(),
        kill,
        reply,
    })
    .ok()?;
    rx.await.ok().flatten()
}

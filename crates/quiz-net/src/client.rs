use crate::error::NetError;
use crate::io::{read_line_limited, write_line, writer_loop};
use crate::protocol::{ClientMsg, ErrorCode, ServerMsg, PROTOCOL_VERSION};
use crate::Waker;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    Message(ServerMsg),
    Disconnected,
}

pub struct ClientHandle {
    pub events: UnboundedReceiver<ClientEvent>,
    player_id: u32,
    out: Option<UnboundedSender<ClientMsg>>,
}

impl ClientHandle {
    pub fn player_id(&self) -> u32 {
        self.player_id
    }

    pub fn answer(&self, question_id: u32, choice: usize) {
        if let Some(out) = &self.out {
            let _ = out.send(ClientMsg::Answer {
                question_id,
                choice,
            });
        }
    }

    /// Dropping the sender closes the write half; the host sees EOF and drops us.
    pub fn disconnect(&mut self) {
        self.out = None;
    }
}

pub struct Client;

impl Client {
    pub async fn connect(
        addr: impl ToSocketAddrs,
        name: &str,
        waker: Option<Waker>,
    ) -> Result<ClientHandle, NetError> {
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| NetError::Timeout)??;
        let (r, mut w) = stream.into_split();
        let mut reader = BufReader::new(r);
        write_line(
            &mut w,
            &ClientMsg::Join {
                name: name.to_string(),
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await?;
        let first = tokio::time::timeout(CONNECT_TIMEOUT, read_line_limited(&mut reader))
            .await
            .map_err(|_| NetError::Timeout)??
            .ok_or_else(|| NetError::Protocol("connexion fermée par l'hôte".into()))?;
        let player_id = match serde_json::from_str::<ServerMsg>(&first) {
            Ok(ServerMsg::Welcome {
                protocol_version,
                player_id,
            }) if protocol_version == PROTOCOL_VERSION => player_id,
            Ok(ServerMsg::Welcome {
                protocol_version, ..
            }) => {
                let _ = protocol_version;
                return Err(NetError::Rejected(ErrorCode::VersionMismatch));
            }
            Ok(ServerMsg::Error { code, .. }) => return Err(NetError::Rejected(code)),
            _ => return Err(NetError::Protocol("réponse inattendue à Join".into())),
        };

        let (out_tx, out_rx) = unbounded_channel::<ClientMsg>();
        tokio::spawn(writer_loop(w, out_rx));
        let (ev_tx, ev_rx) = unbounded_channel();
        tokio::spawn(async move {
            let emit = |ev: ClientEvent| {
                let ok = ev_tx.send(ev).is_ok();
                if let Some(w) = &waker {
                    w();
                }
                ok
            };
            while let Ok(Some(line)) = read_line_limited(&mut reader).await {
                match serde_json::from_str::<ServerMsg>(&line) {
                    Ok(m) => {
                        if !emit(ClientEvent::Message(m)) {
                            return;
                        }
                    }
                    Err(e) => tracing::debug!("message hôte invalide ignoré : {e}"),
                }
            }
            emit(ClientEvent::Disconnected);
        });
        Ok(ClientHandle {
            events: ev_rx,
            player_id,
            out: Some(out_tx),
        })
    }
}

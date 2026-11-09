use crate::discovery::{announce_loop, AnnounceTarget};
use crate::error::NetError;
use crate::game::{Advance, Game, GameError, PlayerId, RevealInfo, HOST_ID};
use crate::io::{enable_keepalive, read_line_limited, writer_loop, Beat};
use crate::protocol::{
    ClientBeat, ClientMsg, ErrorCode, PlayerInfo, PublicQuestion, RankEntry, ServerMsg,
    DEFAULT_PORT, DISCOVERY_PORT, PING_LINE, PROTOCOL_VERSION,
};
use crate::{HeartbeatConfig, Waker};
use quiz_core::Question;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

// A connection that does not send `Join` in time is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct HostConfig {
    pub name: String,
    pub port: u16,
    pub time_limit: Option<Duration>,
    pub announce: bool,
    pub discovery_port: u16,
    pub announce_ip: IpAddr,
    pub waker: Option<Waker>,
    pub heartbeat: crate::HeartbeatConfig,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            name: "Hôte".into(),
            port: DEFAULT_PORT,
            time_limit: Some(Duration::from_secs(20)),
            announce: true,
            discovery_port: DISCOVERY_PORT,
            announce_ip: IpAddr::V4(Ipv4Addr::BROADCAST),
            waker: None,
            heartbeat: crate::HeartbeatConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    LobbyChanged(Vec<PlayerInfo>),
    QuestionStarted(PublicQuestion),
    AnswerProgress { answered: usize, expected: usize },
    Revealed(RevealInfo),
    Ended(Vec<RankEntry>),
    BackToLobby,
    Error(String),
}

enum Cmd {
    Join {
        name: String,
        version: u32,
        out: UnboundedSender<ServerMsg>,
        reply: oneshot::Sender<Option<PlayerId>>,
    },
    Answer {
        id: PlayerId,
        question_id: u32,
        choice: usize,
    },
    Disconnected(PlayerId),
    Start(Vec<Question>, Option<Duration>),
    HostAnswer(usize),
    Next,
    ForceReveal,
    BackToLobby,
    Shutdown,
}

pub struct HostHandle {
    pub events: UnboundedReceiver<HostEvent>,
    cmd: UnboundedSender<Cmd>,
    port: u16,
}

impl HostHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start_game(&self, questions: Vec<Question>, time_limit: Option<Duration>) {
        let _ = self.cmd.send(Cmd::Start(questions, time_limit));
    }

    pub fn host_answer(&self, choice: usize) {
        let _ = self.cmd.send(Cmd::HostAnswer(choice));
    }

    pub fn next_question(&self) {
        let _ = self.cmd.send(Cmd::Next);
    }

    pub fn force_reveal(&self) {
        let _ = self.cmd.send(Cmd::ForceReveal);
    }

    pub fn back_to_lobby(&self) {
        let _ = self.cmd.send(Cmd::BackToLobby);
    }

    pub fn shutdown(&self) {
        let _ = self.cmd.send(Cmd::Shutdown);
    }
}

impl Drop for HostHandle {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Shutdown);
    }
}

pub struct Host;

impl Host {
    pub async fn start(config: HostConfig) -> Result<HostHandle, NetError> {
        let mut game = Game::new(config.time_limit);
        game.add_player(&config.name)?;
        let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
        let port = listener.local_addr()?.port();

        let (cmd_tx, cmd_rx) = unbounded_channel();
        let (ev_tx, ev_rx) = unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);

        // Announcement stays on for the whole session since joining is always allowed.
        let announcer = config.announce.then(|| {
            let target = if config.announce_ip == IpAddr::V4(Ipv4Addr::BROADCAST) {
                AnnounceTarget::Lan(config.discovery_port)
            } else {
                AnnounceTarget::Fixed(SocketAddr::new(config.announce_ip, config.discovery_port))
            };
            tokio::spawn(announce_loop(config.name.clone(), port, target))
        });

        tokio::spawn(accept_loop(
            listener,
            cmd_tx.clone(),
            stop_rx,
            config.heartbeat,
        ));
        let actor = Actor {
            game,
            conns: HashMap::new(),
            events: ev_tx,
            waker: config.waker,
            base: Instant::now(),
            announcer,
        };
        tokio::spawn(actor.run(cmd_rx, stop_tx));
        Ok(HostHandle {
            events: ev_rx,
            cmd: cmd_tx,
            port,
        })
    }
}

async fn accept_loop(
    listener: TcpListener,
    cmd: UnboundedSender<Cmd>,
    mut stop: watch::Receiver<bool>,
    hb: HeartbeatConfig,
) {
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            res = listener.accept() => match res {
                Ok((stream, peer)) => {
                    tracing::debug!(%peer, "incoming connection");
                    enable_keepalive(&stream);
                    tokio::spawn(serve_conn(stream, cmd.clone(), stop.clone(), hb));
                }
                Err(e) => {
                    tracing::warn!("accept: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

async fn serve_conn(
    stream: TcpStream,
    cmd: UnboundedSender<Cmd>,
    mut stop: watch::Receiver<bool>,
    hb: HeartbeatConfig,
) {
    let (r, w) = stream.into_split();
    let (out_tx, out_rx) = unbounded_channel::<ServerMsg>();
    let writer = tokio::spawn(writer_loop(w, out_rx, Beat::every(hb.interval, PING_LINE)));
    let mut reader = BufReader::new(r);

    let id = match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake(&mut reader, &out_tx, &cmd))
        .await
    {
        Ok(Some(id)) => id,
        _ => {
            // Rejected: the actor already queued the error; let the writer flush it before closing.
            drop(out_tx);
            let _ = writer.await;
            return;
        }
    };

    loop {
        let line = tokio::select! {
            _ = stop.changed() => break,
            // Anything received (pongs included) proves the player is alive.
            l = tokio::time::timeout(hb.idle_timeout, read_line_limited(&mut reader)) => l,
        };
        let Ok(Ok(Some(line))) = line else { break };
        match serde_json::from_str::<ClientMsg>(&line) {
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
            Err(_) if serde_json::from_str::<ClientBeat>(&line).is_ok() => {}
            Ok(ClientMsg::Join { .. }) | Err(_) => {
                let _ = out_tx.send(error_msg(ErrorCode::InvalidMessage));
                break;
            }
        }
    }
    let _ = cmd.send(Cmd::Disconnected(id));
}

async fn handshake(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    out: &UnboundedSender<ServerMsg>,
    cmd: &UnboundedSender<Cmd>,
) -> Option<PlayerId> {
    let reject = |code: ErrorCode| {
        let _ = out.send(error_msg(code));
    };
    let Ok(Some(line)) = read_line_limited(reader).await else {
        return None;
    };
    let Ok(ClientMsg::Join {
        name,
        protocol_version,
    }) = serde_json::from_str::<ClientMsg>(&line)
    else {
        reject(ErrorCode::ExpectedJoin);
        return None;
    };
    let (reply, rx) = oneshot::channel();
    cmd.send(Cmd::Join {
        name,
        version: protocol_version,
        out: out.clone(),
        reply,
    })
    .ok()?;
    rx.await.ok().flatten()
}

fn error_msg(code: ErrorCode) -> ServerMsg {
    ServerMsg::Error {
        code,
        message: code.message(quiz_core::Lang::Fr).to_string(),
    }
}

// A single task owns `Game` and all connection senders, so no locks are needed.
struct Actor {
    game: Game,
    conns: HashMap<PlayerId, UnboundedSender<ServerMsg>>,
    events: UnboundedSender<HostEvent>,
    waker: Option<Waker>,
    base: Instant,
    announcer: Option<JoinHandle<()>>,
}

async fn wait_deadline(base: Instant, deadline_ms: Option<u64>) {
    match deadline_ms {
        Some(ms) => tokio::time::sleep_until(base + Duration::from_millis(ms)).await,
        None => std::future::pending().await,
    }
}

impl Actor {
    async fn run(mut self, mut cmd_rx: UnboundedReceiver<Cmd>, stop: watch::Sender<bool>) {
        self.emit(HostEvent::LobbyChanged(self.game.lobby()));
        loop {
            let deadline = self.game.deadline_ms();
            tokio::select! {
                c = cmd_rx.recv() => match c {
                    None | Some(Cmd::Shutdown) => break,
                    Some(c) => self.handle(c),
                },
                _ = wait_deadline(self.base, deadline) => self.reveal(),
            }
        }
        if let Some(a) = self.announcer.take() {
            a.abort();
        }
        let _ = stop.send(true);
    }

    fn now_ms(&self) -> u64 {
        self.base.elapsed().as_millis() as u64
    }

    fn emit(&self, ev: HostEvent) {
        let _ = self.events.send(ev);
        if let Some(w) = &self.waker {
            w();
        }
    }

    fn broadcast(&self, msg: ServerMsg) {
        for tx in self.conns.values() {
            let _ = tx.send(msg.clone());
        }
    }

    fn send_to(&self, id: PlayerId, msg: ServerMsg) {
        if let Some(tx) = self.conns.get(&id) {
            let _ = tx.send(msg);
        }
    }

    fn lobby_changed(&self) {
        let players = self.game.lobby();
        self.broadcast(ServerMsg::Lobby {
            players: players.clone(),
            phase: self.game.phase(),
        });
        self.emit(HostEvent::LobbyChanged(players));
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Join {
                name,
                version,
                out,
                reply,
            } => {
                let result = if version != PROTOCOL_VERSION {
                    Err(ErrorCode::VersionMismatch)
                } else {
                    self.game.add_player(&name).map_err(ErrorCode::from)
                };
                match result {
                    Ok(id) => {
                        let _ = out.send(ServerMsg::Welcome {
                            protocol_version: PROTOCOL_VERSION,
                            player_id: id,
                        });
                        self.conns.insert(id, out);
                        let _ = reply.send(Some(id));
                        self.lobby_changed();
                        // Late joiner: gets the running question (with remaining time) or the final ranking;
                        // during a reveal they simply wait for the next question.
                        if let Some(q) = self.game.current_public(self.now_ms()) {
                            self.send_to(id, ServerMsg::Question(q));
                            self.progress();
                        } else if self.game.is_ended() {
                            self.send_to(
                                id,
                                ServerMsg::End {
                                    ranking: self.game.ranking(),
                                },
                            );
                        }
                    }
                    Err(code) => {
                        let _ = out.send(error_msg(code));
                        let _ = reply.send(None);
                    }
                }
            }
            Cmd::Answer {
                id,
                question_id,
                choice,
            } => self.record_answer(id, question_id, choice),
            Cmd::HostAnswer(choice) => {
                if let Some(q) = self.game.current_question_id() {
                    self.record_answer(HOST_ID, q, choice);
                }
            }
            Cmd::Disconnected(id) => {
                self.conns.remove(&id);
                if self.game.remove_player(id) {
                    self.lobby_changed();
                    if self.game.in_question() {
                        self.progress();
                        self.reveal_if_complete();
                    }
                }
            }
            Cmd::Start(questions, limit) => {
                self.game.set_time_limit(limit);
                match self.game.start(questions, self.now_ms()) {
                    Ok(q) => {
                        self.broadcast(ServerMsg::Question(q.clone()));
                        self.emit(HostEvent::QuestionStarted(q));
                        self.progress();
                    }
                    Err(e @ (GameError::AlreadyStarted | GameError::NoQuestions)) => {
                        self.emit(HostEvent::Error(e.to_string()));
                    }
                }
            }
            Cmd::Next => match self.game.advance(self.now_ms()) {
                Some(Advance::Question(q)) => {
                    self.broadcast(ServerMsg::Question(q.clone()));
                    self.emit(HostEvent::QuestionStarted(q));
                    self.progress();
                }
                Some(Advance::End(ranking)) => {
                    self.broadcast(ServerMsg::End {
                        ranking: ranking.clone(),
                    });
                    self.emit(HostEvent::Ended(ranking));
                }
                None => tracing::debug!("« Suivant » ignoré hors révélation"),
            },
            Cmd::ForceReveal => self.reveal(),
            Cmd::BackToLobby => {
                if self.game.back_to_lobby() {
                    self.broadcast(ServerMsg::BackToLobby);
                    self.emit(HostEvent::BackToLobby);
                    self.lobby_changed();
                } else {
                    self.emit(HostEvent::Error(
                        "retour au lobby impossible : la partie n'est pas terminée".into(),
                    ));
                }
            }
            Cmd::Shutdown => {}
        }
    }

    fn record_answer(&mut self, id: PlayerId, question_id: u32, choice: usize) {
        if self.game.answer(id, question_id, choice, self.now_ms()) {
            self.progress();
            self.reveal_if_complete();
        }
    }

    fn progress(&self) {
        let (answered, expected) = self.game.answer_progress();
        self.emit(HostEvent::AnswerProgress { answered, expected });
    }

    fn reveal_if_complete(&mut self) {
        if self.game.in_question() && self.game.all_answered() {
            self.reveal();
        }
    }

    fn reveal(&mut self) {
        if let Some(info) = self.game.reveal() {
            self.broadcast(ServerMsg::Reveal {
                question_id: info.question_id,
                correct: info.correct,
                explanation: info.explanation.clone(),
                scores: info.scores.clone(),
            });
            self.emit(HostEvent::Revealed(info));
        }
    }
}

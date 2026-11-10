use crate::conn::{serve, Link};
use crate::discovery::{announce_loop, AnnounceTarget};
use crate::error::NetError;
use crate::game::{Advance, Game, GameError, PlayerId, RevealInfo, HOST_ID};
use crate::io::{enable_keepalive, read_line_limited, writer_loop, Beat};
use crate::protocol::{
    Device, ErrorCode, PlayerInfo, PublicQuestion, RankEntry, ServerMsg, DEFAULT_PORT,
    DISCOVERY_PORT, PING_LINE, PROTOCOL_VERSION,
};
use crate::web::{self, WebAssets};
use crate::{HeartbeatConfig, Waker};
use quiz_core::Question;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, watch, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub const DEFAULT_WEB_PORT: u16 = 7880;

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
    /// `None` disables the browser transport; `Some(0)` picks a free port.
    pub web_port: Option<u16>,
    pub web_assets: WebAssets,
    /// How long a disconnected player keeps their seat and score for a resume.
    pub resume_grace: Duration,
    /// Simultaneous TCP + web connections accepted; extra ones are dropped immediately.
    pub max_connections: usize,
    /// Deadline for a browser connection to send its request headers.
    pub http_timeout: Duration,
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
            web_port: Some(DEFAULT_WEB_PORT),
            web_assets: WebAssets::default(),
            resume_grace: Duration::from_secs(600),
            max_connections: 64,
            http_timeout: Duration::from_secs(10),
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

pub(crate) enum Cmd {
    Join {
        name: String,
        version: u32,
        resume: Option<String>,
        device: Device,
        conn_id: u64,
        out: UnboundedSender<ServerMsg>,
        // Dropping it tells the connection task to stop (session taken over).
        kill: oneshot::Sender<()>,
        reply: oneshot::Sender<Option<PlayerId>>,
    },
    Answer {
        id: PlayerId,
        question_id: u32,
        choice: usize,
    },
    Disconnected {
        id: PlayerId,
        conn_id: u64,
    },
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
    web_port: Option<u16>,
}

impl HostHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// `None` when the web transport is disabled or its port could not be bound.
    pub fn web_port(&self) -> Option<u16> {
        self.web_port
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

        let permits = Arc::new(Semaphore::new(config.max_connections));
        let web_port = match config.web_port {
            Some(wp) => match TcpListener::bind(("0.0.0.0", wp)).await {
                Ok(l) => {
                    let bound = l.local_addr().ok().map(|a| a.port());
                    let app = web::router(web::WebState {
                        assets: Arc::new(config.web_assets),
                        cmd: cmd_tx.clone(),
                        stop: stop_rx.clone(),
                        hb: config.heartbeat,
                    });
                    tokio::spawn(web::serve_http(
                        l,
                        app,
                        permits.clone(),
                        stop_rx.clone(),
                        config.http_timeout,
                    ));
                    bound
                }
                Err(e) => {
                    tracing::warn!("web server disabled, cannot bind port {wp}: {e}");
                    None
                }
            },
            None => None,
        };

        tokio::spawn(accept_loop(
            listener,
            cmd_tx.clone(),
            stop_rx,
            config.heartbeat,
            permits,
        ));
        let actor = Actor {
            game,
            conns: HashMap::new(),
            grace_ms: config.resume_grace.as_millis() as u64,
            last_reveal: None,
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
            web_port,
        })
    }
}

async fn accept_loop(
    listener: TcpListener,
    cmd: UnboundedSender<Cmd>,
    mut stop: watch::Receiver<bool>,
    hb: HeartbeatConfig,
    permits: Arc<Semaphore>,
) {
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            res = listener.accept() => match res {
                Ok((stream, peer)) => {
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        tracing::debug!(%peer, "connection limit reached");
                        continue;
                    };
                    tracing::debug!(%peer, "incoming connection");
                    enable_keepalive(&stream);
                    let (cmd, stop) = (cmd.clone(), stop.clone());
                    tokio::spawn(async move {
                        serve_conn(stream, cmd, stop, hb).await;
                        drop(permit);
                    });
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
    stop: watch::Receiver<bool>,
    hb: HeartbeatConfig,
) {
    let (r, w) = stream.into_split();
    let (out, out_rx) = unbounded_channel::<ServerMsg>();
    tokio::spawn(writer_loop(w, out_rx, Beat::every(hb.interval, PING_LINE)));
    let (in_tx, inbound) = mpsc::channel(32);
    tokio::spawn(tcp_pump(BufReader::new(r), in_tx));
    serve(
        Link {
            inbound,
            out,
            device: Device::App,
        },
        cmd,
        stop,
        hb,
    )
    .await;
}

// Dropping `tx` (EOF, oversized or invalid line) is how the connection task learns the peer is gone.
async fn tcp_pump(mut reader: BufReader<OwnedReadHalf>, tx: mpsc::Sender<String>) {
    loop {
        tokio::select! {
            _ = tx.closed() => break,
            l = read_line_limited(&mut reader) => match l {
                Ok(Some(l)) => {
                    if tx.send(l).await.is_err() {
                        break;
                    }
                }
                _ => break,
            },
        }
    }
}

pub(crate) fn error_msg(code: ErrorCode) -> ServerMsg {
    ServerMsg::Error {
        code,
        message: code.message(quiz_core::Lang::Fr).to_string(),
    }
}

// A single task owns `Game` and all connection senders, so no locks are needed.
struct Conn {
    id: u64,
    out: UnboundedSender<ServerMsg>,
    _kill: oneshot::Sender<()>,
}

struct Actor {
    game: Game,
    conns: HashMap<PlayerId, Conn>,
    grace_ms: u64,
    last_reveal: Option<ServerMsg>,
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
            let expiry = self.game.next_expiry_ms(self.grace_ms);
            tokio::select! {
                c = cmd_rx.recv() => match c {
                    None | Some(Cmd::Shutdown) => break,
                    Some(c) => self.handle(c),
                },
                _ = wait_deadline(self.base, deadline) => self.reveal(),
                _ = wait_deadline(self.base, expiry) => {
                    if self.game.expire_disconnected(self.now_ms(), self.grace_ms) {
                        self.lobby_changed();
                    }
                }
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
        for c in self.conns.values() {
            let _ = c.out.send(msg.clone());
        }
    }

    fn send_to(&self, id: PlayerId, msg: ServerMsg) {
        if let Some(c) = self.conns.get(&id) {
            let _ = c.out.send(msg);
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
                resume,
                device,
                conn_id,
                out,
                kill,
                reply,
            } => {
                let admitted = if version != PROTOCOL_VERSION {
                    Err(ErrorCode::VersionMismatch)
                } else {
                    self.admit(&name, resume.as_deref(), device)
                };
                match admitted {
                    Ok((id, resumed, token)) => {
                        let _ = out.send(ServerMsg::Welcome {
                            protocol_version: PROTOCOL_VERSION,
                            player_id: id,
                            resume_token: token,
                        });
                        // Replacing an entry drops the old sender and kill switch: last connection wins.
                        self.conns.insert(
                            id,
                            Conn {
                                id: conn_id,
                                out,
                                _kill: kill,
                            },
                        );
                        let _ = reply.send(Some(id));
                        self.lobby_changed();
                        // Late joiner or resumed player: gets the running question (with remaining
                        // time) or the final ranking. A resumed player also gets the current
                        // reveal; a new one simply waits for the next question.
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
                        } else if resumed && self.game.in_reveal() {
                            if let Some(r) = self.last_reveal.clone() {
                                self.send_to(id, r);
                            }
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
            Cmd::Disconnected { id, conn_id } => {
                // A replaced connection reports in late: it must not evict its successor.
                if self.conns.get(&id).is_none_or(|c| c.id != conn_id) {
                    return;
                }
                self.conns.remove(&id);
                if self.game.disconnect(id, self.now_ms()) {
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

    /// Returns (player id, resumed?, fresh resume token).
    fn admit(
        &mut self,
        name: &str,
        resume: Option<&str>,
        device: Device,
    ) -> Result<(PlayerId, bool, String), ErrorCode> {
        let token = new_token().ok_or(ErrorCode::Internal)?;
        let resumed = match resume {
            Some(t) => self.game.resume(t, device).map_err(ErrorCode::from)?,
            None => None,
        };
        let (id, was_resumed) = match resumed {
            Some(id) => (id, true),
            None => (
                self.game
                    .add_player_on(name, device)
                    .map_err(ErrorCode::from)?,
                false,
            ),
        };
        self.game.set_token(id, token.clone());
        Ok((id, was_resumed, token))
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
            let msg = ServerMsg::Reveal {
                question_id: info.question_id,
                correct: info.correct,
                explanation: info.explanation.clone(),
                scores: info.scores.clone(),
            };
            self.last_reveal = Some(msg.clone());
            self.broadcast(msg);
            self.emit(HostEvent::Revealed(info));
        }
    }
}

fn new_token() -> Option<String> {
    use std::fmt::Write;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

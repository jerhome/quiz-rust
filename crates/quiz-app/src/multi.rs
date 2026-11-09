use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use quiz_core::Question;

use crate::i18n::{tf, Key, Msg};
use quiz_net::game::RevealInfo;
use quiz_net::{
    local_ipv4_addrs, ClientEvent, ClientHandle, DiscoveredHost, GamePhase, HostEvent, HostHandle,
    LocalAddr, PlayerInfo, PublicQuestion, RankEntry, ScoreEntry, ServerMsg, DEFAULT_PORT, HOST_ID,
};

// Discovery has no "host gone" event: hosts not seen for this long are hidden.
pub const HOST_TTL: Duration = Duration::from_secs(6);
pub const MAX_NAME_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MPhase {
    Lobby,
    Question,
    Reveal,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealView {
    pub question_id: u32,
    pub correct: usize,
    pub explanation: String,
    pub scores: Vec<ScoreEntry>,
}

impl From<RevealInfo> for RevealView {
    fn from(r: RevealInfo) -> Self {
        Self {
            question_id: r.question_id,
            correct: r.correct,
            explanation: r.explanation,
            scores: r.scores,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetUpdate {
    Lobby {
        players: Vec<PlayerInfo>,
        phase: Option<GamePhase>,
    },
    Question(PublicQuestion),
    Progress {
        answered: usize,
        expected: usize,
    },
    Reveal(RevealView),
    End(Vec<RankEntry>),
    BackToLobby,
    Disconnected,
    Fatal(Msg),
    Ignore,
}

pub fn from_host_event(e: HostEvent) -> NetUpdate {
    match e {
        HostEvent::LobbyChanged(players) => NetUpdate::Lobby {
            players,
            phase: None,
        },
        HostEvent::QuestionStarted(q) => NetUpdate::Question(q),
        HostEvent::AnswerProgress { answered, expected } => {
            NetUpdate::Progress { answered, expected }
        }
        HostEvent::Revealed(r) => NetUpdate::Reveal(r.into()),
        HostEvent::Ended(r) => NetUpdate::End(r),
        HostEvent::BackToLobby => NetUpdate::BackToLobby,
        HostEvent::Error(m) => {
            NetUpdate::Fatal(Msg::from_fn(|l| tf(l, Key::ErrHost, &[m.as_str()])))
        }
    }
}

pub fn from_server_msg(m: ServerMsg) -> NetUpdate {
    match m {
        ServerMsg::Welcome { .. } => NetUpdate::Ignore,
        ServerMsg::Error { code, .. } => NetUpdate::Fatal(code_message(code)),
        ServerMsg::Lobby { players, phase } => NetUpdate::Lobby {
            players,
            phase: Some(phase),
        },
        ServerMsg::Question(q) => NetUpdate::Question(q),
        ServerMsg::Reveal {
            question_id,
            correct,
            explanation,
            scores,
        } => NetUpdate::Reveal(RevealView {
            question_id,
            correct,
            explanation,
            scores,
        }),
        ServerMsg::End { ranking } => NetUpdate::End(ranking),
        ServerMsg::BackToLobby => NetUpdate::BackToLobby,
    }
}

pub fn from_client_event(e: ClientEvent) -> NetUpdate {
    match e {
        ClientEvent::Message(m) => from_server_msg(m),
        ClientEvent::Disconnected => NetUpdate::Disconnected,
    }
}

enum Link {
    Host(HostHandle),
    Client(ClientHandle),
    #[cfg(test)]
    Detached,
}

pub struct Session {
    link: Link,
    pub players: Vec<PlayerInfo>,
    pub phase: MPhase,
    pub question: Option<PublicQuestion>,
    deadline: Option<Instant>,
    total: Option<Duration>,
    pub my_choice: Option<usize>,
    pub progress: Option<(usize, usize)>,
    pub reveal: Option<RevealView>,
    pub ranking: Vec<RankEntry>,
    pub my_id: u32,
    questions: Vec<Question>,
    pub launched: bool,
    pub game_phase: Option<GamePhase>,
    time_limit: Option<Duration>,
    pub port: u16,
    pub addrs: Vec<LocalAddr>,
}

impl Session {
    fn new(link: Link, my_id: u32, port: u16) -> Self {
        Self {
            link,
            players: Vec::new(),
            phase: MPhase::Lobby,
            question: None,
            deadline: None,
            total: None,
            my_choice: None,
            progress: None,
            reveal: None,
            ranking: Vec::new(),
            my_id,
            questions: Vec::new(),
            launched: false,
            game_phase: None,
            time_limit: None,
            port,
            addrs: Vec::new(),
        }
    }

    pub fn new_host(
        handle: HostHandle,
        name: &str,
        questions: Vec<Question>,
        time_limit: Option<Duration>,
    ) -> Self {
        let port = handle.port();
        let mut s = Self::new(Link::Host(handle), HOST_ID, port);
        s.players = vec![PlayerInfo {
            id: HOST_ID,
            name: name.to_string(),
            is_host: true,
        }];
        s.questions = questions;
        s.time_limit = time_limit;
        s.addrs = local_ipv4_addrs();
        s
    }

    pub fn new_client(handle: ClientHandle) -> Self {
        let id = handle.player_id();
        Self::new(Link::Client(handle), id, 0)
    }

    pub fn is_host(&self) -> bool {
        matches!(self.link, Link::Host(_))
    }

    pub fn start_game(&mut self) {
        if let (Link::Host(h), false) = (&self.link, self.launched) {
            h.start_game(std::mem::take(&mut self.questions), self.time_limit);
            self.launched = true;
        }
    }

    pub fn next_question(&self) {
        if let Link::Host(h) = &self.link {
            h.next_question();
        }
    }

    pub fn answer(&mut self, choice: usize) {
        let Some(q) = &self.question else { return };
        if self.my_choice.is_some() || self.phase != MPhase::Question {
            return;
        }
        match &self.link {
            Link::Host(h) => h.host_answer(choice),
            Link::Client(c) => c.answer(q.id, choice),
            #[cfg(test)]
            Link::Detached => {}
        }
        self.my_choice = Some(choice);
    }

    pub fn remaining_secs(&self, now: Instant) -> Option<u32> {
        self.deadline
            .map(|d| remaining_secs(d.saturating_duration_since(now)))
    }

    pub fn time_fraction(&self, now: Instant) -> Option<f32> {
        let left = self.deadline?.saturating_duration_since(now);
        Some(fraction_left(left, self.total?))
    }

    pub fn force_reveal(&self) {
        if let Link::Host(h) = &self.link {
            h.force_reveal();
        }
    }

    pub fn back_to_lobby(&self) {
        if let Link::Host(h) = &self.link {
            h.back_to_lobby();
        }
    }

    pub fn set_questions(&mut self, questions: Vec<Question>, time_limit: Option<Duration>) {
        self.questions = questions;
        self.time_limit = time_limit;
        self.launched = false;
    }

    pub fn joined_mid_game(&self) -> bool {
        self.phase == MPhase::Lobby && self.game_phase == Some(GamePhase::Reveal)
    }

    pub fn can_launch(&self) -> bool {
        !self.launched && !self.questions.is_empty()
    }

    pub fn drain(&mut self) -> Option<Msg> {
        let mut updates = Vec::new();
        match &mut self.link {
            Link::Host(h) => {
                while let Ok(e) = h.events.try_recv() {
                    updates.push(from_host_event(e));
                }
            }
            Link::Client(c) => {
                while let Ok(e) = c.events.try_recv() {
                    updates.push(from_client_event(e));
                }
            }
            #[cfg(test)]
            Link::Detached => {}
        }
        updates.into_iter().find_map(|u| self.apply(u))
    }

    pub fn apply(&mut self, update: NetUpdate) -> Option<Msg> {
        match update {
            // The host also broadcasts Lobby mid-game on every join/leave: it only refreshes the
            // player list and phase, it must never change the screen (only BackToLobby does).
            NetUpdate::Lobby { players, phase } => {
                self.players = players;
                self.game_phase = phase;
            }
            NetUpdate::Question(q) => {
                self.phase = MPhase::Question;
                self.deadline = question_deadline(&q, Instant::now());
                self.total = question_total(&q);
                self.my_choice = None;
                self.progress = None;
                self.reveal = None;
                self.question = Some(q);
            }
            NetUpdate::Progress { answered, expected } => {
                self.progress = Some((answered, expected))
            }
            NetUpdate::Reveal(r) => {
                self.phase = MPhase::Reveal;
                self.reveal = Some(r);
            }
            NetUpdate::BackToLobby => {
                self.phase = MPhase::Lobby;
                self.question = None;
                self.reveal = None;
                self.my_choice = None;
                self.progress = None;
                self.deadline = None;
                self.total = None;
                self.ranking.clear();
                self.launched = false;
                self.game_phase = None;
            }
            NetUpdate::End(r) => {
                self.phase = MPhase::End;
                self.ranking = r;
            }
            // A disconnect after the final ranking is the host closing normally, not an error.
            NetUpdate::Disconnected if self.phase != MPhase::End => {
                return Some(Msg::Key(Key::ErrConnLost));
            }
            NetUpdate::Fatal(m) => return Some(m),
            NetUpdate::Disconnected | NetUpdate::Ignore => {}
        }
        None
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        match &mut self.link {
            Link::Host(h) => h.shutdown(),
            Link::Client(c) => c.disconnect(),
            #[cfg(test)]
            Link::Detached => {}
        }
    }
}

// Built from `remaining_ms` so a player joining mid-question sees the right time.
pub fn question_deadline(q: &PublicQuestion, now: Instant) -> Option<Instant> {
    let left = q
        .remaining_ms
        .map(Duration::from_millis)
        .or_else(|| q.time_limit_secs.map(|s| Duration::from_secs(u64::from(s))))?;
    Some(now + left)
}

pub fn question_total(q: &PublicQuestion) -> Option<Duration> {
    q.time_limit_secs
        .map(|s| Duration::from_secs(u64::from(s)))
        .or_else(|| q.remaining_ms.map(Duration::from_millis))
}

pub fn fraction_left(left: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 0.0;
    }
    (left.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
}

pub fn remaining_secs(left: Duration) -> u32 {
    u32::try_from(left.as_secs()).unwrap_or(u32::MAX) + u32::from(left.subsec_nanos() > 0)
}

/// Address to give to players: the first real (non-virtual) interface.
pub fn primary_addr(addrs: &[LocalAddr]) -> Option<&LocalAddr> {
    addrs.iter().find(|a| !a.is_likely_virtual)
}

pub fn format_addr(ip: std::net::Ipv4Addr, port: u16) -> String {
    format!("{ip}:{port}")
}

/// After this long without any discovered game, the join screen suggests typing the address.
pub const NO_GAMES_HINT_AFTER: Duration = Duration::from_secs(5);

pub fn show_no_games_hint(started: Instant, now: Instant, any_host: bool) -> bool {
    !any_host && now.saturating_duration_since(started) >= NO_GAMES_HINT_AFTER
}

/// The host-side text for a version mismatch is replaced by a clearer one.
pub fn code_message(code: quiz_net::ErrorCode) -> Msg {
    if code == quiz_net::ErrorCode::VersionMismatch {
        Msg::Key(Key::ErrVersion)
    } else {
        Msg::from_fn(|l| code.message(l).to_string())
    }
}

pub fn parse_addr(input: &str) -> Result<SocketAddr, Msg> {
    let s = input.trim();
    if let Ok(a) = s.parse::<SocketAddr>() {
        return Ok(a);
    }
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, DEFAULT_PORT));
    }
    Err(Msg::Key(Key::ErrAddr))
}

pub fn validate_name(input: &str) -> Result<String, Msg> {
    let n = input.trim();
    if n.is_empty() {
        Err(Msg::Key(Key::ErrNameRequired))
    } else if n.chars().count() > MAX_NAME_LEN {
        Err(Msg::from_fn(|l| {
            tf(l, Key::ErrNameTooLong, &[&MAX_NAME_LEN.to_string()])
        }))
    } else {
        Ok(n.to_string())
    }
}

// Hosts are re-emitted on every announcement, so the newest entry replaces the old one.
pub fn upsert_host(list: &mut Vec<DiscoveredHost>, host: DiscoveredHost) {
    match list.iter_mut().find(|h| h.addr == host.addr) {
        Some(entry) => *entry = host,
        None => list.push(host),
    }
}

pub fn visible_hosts(list: &[DiscoveredHost], now: Instant, ttl: Duration) -> Vec<&DiscoveredHost> {
    list.iter()
        .filter(|h| now.saturating_duration_since(h.last_seen) <= ttl)
        .collect()
}

pub fn is_tied(ranking: &[RankEntry], entry: &RankEntry) -> bool {
    ranking
        .iter()
        .any(|r| r.rank == entry.rank && r.player_id != entry.player_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str, port: u16, last_seen: Instant) -> DiscoveredHost {
        DiscoveredHost {
            name: name.into(),
            addr: SocketAddr::new([192, 168, 1, 5].into(), port),
            last_seen,
        }
    }

    #[test]
    fn adresse_valide_ou_non() {
        assert_eq!(parse_addr(" 10.0.0.2:9000 ").unwrap().port(), 9000);
        assert_eq!(parse_addr("10.0.0.2").unwrap().port(), DEFAULT_PORT);
        assert!(parse_addr("abc").is_err());
        assert!(parse_addr("10.0.0.2:xx").is_err());
        assert!(parse_addr("").is_err());
    }

    #[test]
    fn pseudo() {
        assert_eq!(validate_name("  Zoé ").unwrap(), "Zoé");
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn hotes_expires_masques() {
        let now = Instant::now();
        let mut list = Vec::new();
        upsert_host(&mut list, host("A", 1, now));
        upsert_host(&mut list, host("B", 2, now + Duration::from_secs(5)));
        let later = now + Duration::from_secs(8);
        let v = visible_hosts(&list, later, HOST_TTL);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "B");
        // a new announcement refreshes last_seen and keeps a single entry
        upsert_host(&mut list, host("A2", 1, later));
        assert_eq!(list.len(), 2);
        assert_eq!(visible_hosts(&list, later, HOST_TTL).len(), 2);
    }

    fn la(ip: [u8; 4], name: &str, virt: bool) -> LocalAddr {
        LocalAddr {
            ip: ip.into(),
            interface_name: name.into(),
            is_likely_virtual: virt,
        }
    }

    #[test]
    fn adresse_principale() {
        let list = vec![
            la([10, 8, 0, 2], "tun0", true),
            la([192, 168, 1, 20], "eth0", false),
        ];
        assert_eq!(primary_addr(&list).unwrap().interface_name, "eth0");
        assert!(primary_addr(&[la([10, 8, 0, 2], "tun0", true)]).is_none());
        assert!(primary_addr(&[]).is_none());
        assert_eq!(
            format_addr([192, 168, 1, 20].into(), 7878),
            "192.168.1.20:7878"
        );
    }

    #[test]
    fn aide_aucune_partie() {
        let start = Instant::now();
        let at = |s| start + Duration::from_secs(s);
        assert!(!show_no_games_hint(start, at(4), false));
        assert!(show_no_games_hint(start, at(5), false));
        assert!(!show_no_games_hint(start, at(30), true));
    }

    #[test]
    fn message_version_differente() {
        let m = code_message(quiz_net::ErrorCode::VersionMismatch);
        assert!(m.text(quiz_core::Lang::Fr).contains("Version différente"));
        assert!(m.text(quiz_core::Lang::En).contains("Different version"));
        assert_ne!(code_message(quiz_net::ErrorCode::NameTaken), m);
    }

    fn pq(limit: Option<u32>, remaining_ms: Option<u64>) -> PublicQuestion {
        PublicQuestion {
            id: 1,
            index: 0,
            total: 5,
            prompt: "p".into(),
            choices: vec![],
            time_limit_secs: limit,
            remaining_ms,
        }
    }

    #[test]
    fn compte_a_rebours() {
        assert_eq!(remaining_secs(Duration::from_secs(20)), 20);
        assert_eq!(remaining_secs(Duration::from_millis(19_500)), 20);
        assert_eq!(remaining_secs(Duration::ZERO), 0);
        let now = Instant::now();
        let d = question_deadline(&pq(Some(20), Some(7_000)), now).unwrap();
        assert_eq!(d - now, Duration::from_secs(7));
        let d = question_deadline(&pq(Some(20), None), now).unwrap();
        assert_eq!(d - now, Duration::from_secs(20));
        assert!(question_deadline(&pq(None, None), now).is_none());
    }

    #[test]
    fn fraction_de_temps() {
        let total = Duration::from_secs(20);
        assert_eq!(fraction_left(Duration::from_secs(10), total), 0.5);
        assert_eq!(fraction_left(Duration::from_secs(40), total), 1.0);
        assert_eq!(fraction_left(Duration::ZERO, Duration::ZERO), 0.0);
        assert_eq!(question_total(&pq(None, None)), None);
        assert_eq!(question_total(&pq(Some(20), Some(5))), Some(total));
    }

    #[test]
    fn mapping_evenements() {
        assert_eq!(
            from_host_event(HostEvent::AnswerProgress {
                answered: 1,
                expected: 3
            }),
            NetUpdate::Progress {
                answered: 1,
                expected: 3
            }
        );
        assert!(matches!(
            from_host_event(HostEvent::Error("x".into())),
            NetUpdate::Fatal(_)
        ));
        assert_eq!(
            from_client_event(ClientEvent::Disconnected),
            NetUpdate::Disconnected
        );
        assert_eq!(
            from_server_msg(ServerMsg::Welcome {
                protocol_version: 1,
                player_id: 2
            }),
            NetUpdate::Ignore
        );
        assert_eq!(
            from_server_msg(ServerMsg::BackToLobby),
            NetUpdate::BackToLobby
        );
        assert_eq!(
            from_host_event(HostEvent::BackToLobby),
            NetUpdate::BackToLobby
        );
        assert!(matches!(
            from_server_msg(ServerMsg::Error {
                code: quiz_net::ErrorCode::NameTaken,
                message: "m".into()
            }),
            NetUpdate::Fatal(_)
        ));
    }

    fn detached() -> Session {
        Session::new(Link::Detached, 1, 0)
    }

    #[test]
    fn lobby_en_cours_de_partie_ne_change_pas_l_ecran() {
        let mut s = detached();
        s.apply(NetUpdate::Question(pq(Some(20), Some(5_000))));
        assert_eq!(s.phase, MPhase::Question);
        s.apply(NetUpdate::Lobby {
            players: vec![],
            phase: Some(GamePhase::Question),
        });
        assert_eq!(s.phase, MPhase::Question);
        assert!(s.question.is_some());
    }

    #[test]
    fn arrivee_pendant_revelation() {
        let mut s = detached();
        assert!(!s.joined_mid_game());
        s.apply(NetUpdate::Lobby {
            players: vec![],
            phase: Some(GamePhase::Reveal),
        });
        assert_eq!(s.phase, MPhase::Lobby);
        assert!(s.joined_mid_game());
        s.apply(NetUpdate::Question(pq(None, None)));
        assert!(!s.joined_mid_game());
        s.apply(NetUpdate::Lobby {
            players: vec![],
            phase: Some(GamePhase::Reveal),
        });
        assert_eq!(s.phase, MPhase::Question);
        s.apply(NetUpdate::BackToLobby);
        assert!(!s.joined_mid_game());
    }

    #[test]
    fn retour_au_salon_reinitialise() {
        let mut s = detached();
        s.apply(NetUpdate::Question(pq(None, None)));
        s.apply(NetUpdate::End(vec![]));
        assert_eq!(s.phase, MPhase::End);
        assert!(s.apply(NetUpdate::Disconnected).is_none());
        s.apply(NetUpdate::BackToLobby);
        assert_eq!(s.phase, MPhase::Lobby);
        assert!(s.question.is_none() && s.ranking.is_empty());
        assert!(s.apply(NetUpdate::Disconnected).is_some());
    }

    #[test]
    fn ex_aequo() {
        let r = |rank, id| RankEntry {
            rank,
            player_id: id,
            name: String::new(),
            score: 0,
        };
        let ranking = vec![r(1, 0), r(1, 1), r(3, 2)];
        assert!(is_tied(&ranking, &ranking[0]));
        assert!(!is_tied(&ranking, &ranking[2]));
    }
}

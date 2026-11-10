use crate::game::JoinError;
use quiz_core::{Lang, Question};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Bump on any wire change; the host refuses clients with another version.
pub const PROTOCOL_VERSION: u32 = 5;
pub const DEFAULT_PORT: u16 = 7878;
pub const DISCOVERY_PORT: u16 = 7879;
/// Bounds per-connection memory: a longer line drops the connection.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Browser frames are tiny (join/answer); anything bigger closes the socket.
pub const MAX_WS_FRAME_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Language-independent so each client renders it in its own language;
/// `ServerMsg::Error::message` is only a French fallback.
pub enum ErrorCode {
    VersionMismatch,
    NameTaken,
    NameEmpty,
    NameTooLong,
    LobbyFull,
    ExpectedJoin,
    InvalidMessage,
    /// Host-side failure unrelated to the peer's input (e.g. no system randomness).
    Internal,
}

impl ErrorCode {
    pub fn message(self, lang: Lang) -> &'static str {
        use ErrorCode::*;
        match (self, lang) {
            (VersionMismatch, Lang::Fr) => "version de protocole incompatible",
            (VersionMismatch, Lang::En) => "incompatible protocol version",
            (NameTaken, Lang::Fr) => "ce nom est déjà pris",
            (NameTaken, Lang::En) => "this name is already taken",
            (NameEmpty, Lang::Fr) => "le nom est vide",
            (NameEmpty, Lang::En) => "the name is empty",
            (NameTooLong, Lang::Fr) => "le nom est trop long (24 caractères maximum)",
            (NameTooLong, Lang::En) => "the name is too long (24 characters maximum)",
            (LobbyFull, Lang::Fr) => "la partie est pleine (8 joueurs maximum)",
            (LobbyFull, Lang::En) => "the game is full (8 players maximum)",
            (ExpectedJoin, Lang::Fr) => "message d'inscription attendu",
            (ExpectedJoin, Lang::En) => "join message expected",
            (InvalidMessage, Lang::Fr) => "message invalide",
            (InvalidMessage, Lang::En) => "invalid message",
            (Internal, Lang::Fr) => "erreur interne de l'hôte",
            (Internal, Lang::En) => "internal host error",
        }
    }
}

impl From<JoinError> for ErrorCode {
    fn from(e: JoinError) -> Self {
        match e {
            JoinError::EmptyName => ErrorCode::NameEmpty,
            JoinError::NameTooLong => ErrorCode::NameTooLong,
            JoinError::NameTaken => ErrorCode::NameTaken,
            JoinError::LobbyFull => ErrorCode::LobbyFull,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    Lobby,
    Question,
    Reveal,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    pub is_host: bool,
    #[serde(default)]
    pub device: Device,
    /// `false` while the player is inside the resume grace period.
    #[serde(default = "default_true")]
    pub connected: bool,
}

fn default_true() -> bool {
    true
}

/// Set by the server from the transport the player joined with, never by the client.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    #[default]
    App,
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreEntry {
    pub player_id: u32,
    pub name: String,
    pub score: u32,
    /// Only meaningful inside `Reveal` (0 / `None` elsewhere).
    pub gained: u32,
    pub choice: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankEntry {
    pub rank: u32,
    pub player_id: u32,
    pub name: String,
    pub score: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Anti-cheat: deliberately has no `correct` nor `explanation`; they only travel in
/// `ServerMsg::Reveal`. Keep it that way.
pub struct PublicQuestion {
    pub id: u32,
    pub index: usize,
    pub total: usize,
    pub prompt: String,
    pub choices: Vec<String>,
    pub time_limit_secs: Option<u32>,
    pub remaining_ms: Option<u64>,
}

impl PublicQuestion {
    pub(crate) fn from_question(
        q: &Question,
        index: usize,
        total: usize,
        time_limit_secs: Option<u32>,
        remaining_ms: Option<u64>,
    ) -> Self {
        Self {
            id: q.id,
            index,
            total,
            prompt: q.prompt.clone(),
            choices: q.choices.clone(),
            time_limit_secs,
            remaining_ms,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    Join {
        name: String,
        protocol_version: u32,
        /// Token from a previous `Welcome`; reclaims that player's identity and score.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
    },
    Answer {
        question_id: u32,
        choice: usize,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Welcome {
        protocol_version: u32,
        player_id: u32,
        /// Secret and rotated on every join; must never be logged.
        resume_token: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    /// Also sent mid-game on every join/leave: a plain player-list refresh, not a phase
    /// change. `phase` tells a late joiner where the game is.
    Lobby {
        players: Vec<PlayerInfo>,
        phase: GamePhase,
    },
    Question(PublicQuestion),
    Reveal {
        question_id: u32,
        correct: usize,
        explanation: String,
        scores: Vec<ScoreEntry>,
    },
    End {
        ranking: Vec<RankEntry>,
    },
    /// Explicit phase change after the end; a `Lobby` follows right away.
    BackToLobby,
}

/// Heartbeat lines are handled inside the connection tasks and never surface in
/// `ServerMsg`/`ClientMsg`, so the UI does not have to know about them.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ServerBeat {
    Ping,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ClientBeat {
    Pong,
}

pub(crate) const PING_LINE: &[u8] = b"{\"type\":\"Ping\"}\n";
pub(crate) const PONG_LINE: &[u8] = b"{\"type\":\"Pong\"}\n";

// Debug goes through the JSON form so a new variant cannot forget to mask a secret.
fn fmt_masked<T: Serialize>(v: &T, secret: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut json = serde_json::to_value(v).map_err(|_| fmt::Error)?;
    if let Some(field) = json.get_mut(secret) {
        *field = "***".into();
    }
    write!(f, "{json}")
}

impl fmt::Debug for ClientMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_masked(self, "resume", f)
    }
}

impl fmt::Debug for ServerMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_masked(self, "resume_token", f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_msg_never_contains_answer_or_explanation() {
        let q = Question {
            id: 3,
            prompt: "Capitale ?".into(),
            choices: vec!["A".into(), "B".into(), "C".into(), "D".into()],
            correct: 2,
            explanation: "EXPLICATION_SECRETE".into(),
        };
        let msg = ServerMsg::Question(PublicQuestion::from_question(
            &q,
            0,
            5,
            Some(20),
            Some(20_000),
        ));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("correct"), "{json}");
        assert!(!json.contains("explanation"), "{json}");
        assert!(!json.contains("EXPLICATION_SECRETE"), "{json}");
        assert!(json.contains("Capitale ?"));
        assert!(!json.contains('\n'));
    }

    #[test]
    fn error_codes_have_both_messages_and_roundtrip() {
        use ErrorCode::*;
        for c in [
            VersionMismatch,
            NameTaken,
            NameEmpty,
            NameTooLong,
            LobbyFull,
            ExpectedJoin,
            InvalidMessage,
        ] {
            let (fr, en) = (c.message(Lang::Fr), c.message(Lang::En));
            assert!(!fr.is_empty() && !en.is_empty() && fr != en, "{c:?}");
            let m = ServerMsg::Error {
                code: c,
                message: fr.into(),
            };
            let s = serde_json::to_string(&m).unwrap();
            assert_eq!(serde_json::from_str::<ServerMsg>(&s).unwrap(), m);
        }
    }

    #[test]
    fn join_errors_map_to_codes() {
        assert_eq!(ErrorCode::from(JoinError::NameTaken), ErrorCode::NameTaken);
        assert_eq!(ErrorCode::from(JoinError::EmptyName), ErrorCode::NameEmpty);
        assert_eq!(
            ErrorCode::from(JoinError::NameTooLong),
            ErrorCode::NameTooLong
        );
        assert_eq!(ErrorCode::from(JoinError::LobbyFull), ErrorCode::LobbyFull);
    }

    #[test]
    fn heartbeat_lines_parse_as_beats_not_messages() {
        let ping = std::str::from_utf8(PING_LINE).unwrap();
        assert!(serde_json::from_str::<ServerMsg>(ping).is_err());
        assert!(serde_json::from_str::<ServerBeat>(ping).is_ok());
        let pong = std::str::from_utf8(PONG_LINE).unwrap();
        assert!(serde_json::from_str::<ClientMsg>(pong).is_err());
        assert!(serde_json::from_str::<ClientBeat>(pong).is_ok());
    }

    #[test]
    fn v5_fields_have_backward_compatible_defaults() {
        let j: ClientMsg =
            serde_json::from_str(r#"{"type":"Join","name":"A","protocol_version":5}"#).unwrap();
        assert!(matches!(j, ClientMsg::Join { resume: None, .. }));
        let p: PlayerInfo = serde_json::from_str(r#"{"id":1,"name":"A","is_host":false}"#).unwrap();
        assert_eq!((p.device, p.connected), (Device::App, true));
    }

    #[test]
    fn messages_roundtrip() {
        let m = ClientMsg::Join {
            name: "A".into(),
            protocol_version: 1,
            resume: Some("tok".into()),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<ClientMsg>(&s).unwrap(), m);
        let m = ServerMsg::Welcome {
            protocol_version: 1,
            player_id: 4,
            resume_token: "tok".into(),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<ServerMsg>(&s).unwrap(), m);
    }

    #[test]
    fn debug_hides_resume_tokens() {
        let j = ClientMsg::Join {
            name: "A".into(),
            protocol_version: PROTOCOL_VERSION,
            resume: Some("SECRETTOKEN".into()),
        };
        let w = ServerMsg::Welcome {
            protocol_version: PROTOCOL_VERSION,
            player_id: 1,
            resume_token: "SECRETTOKEN".into(),
        };
        for d in [
            format!("{j:?}"),
            format!("{w:?}"),
            format!("{:?}", Some(&w)),
        ] {
            assert!(!d.contains("SECRETTOKEN"), "{d}");
            assert!(d.contains("***"), "{d}");
        }
        assert!(format!("{j:?}").contains("Join") && format!("{w:?}").contains("Welcome"));
        assert!(serde_json::to_string(&w).unwrap().contains("SECRETTOKEN"));
        let none = ClientMsg::Join {
            name: "A".into(),
            protocol_version: 1,
            resume: None,
        };
        assert!(!format!("{none:?}").contains("***"));
    }

    #[test]
    fn internal_error_code_has_messages() {
        for l in [Lang::Fr, Lang::En] {
            assert!(!ErrorCode::Internal.message(l).is_empty());
        }
    }
}

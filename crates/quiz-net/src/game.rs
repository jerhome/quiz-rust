//! Pure game logic: no I/O. Time is injected as `now_ms` (milliseconds on any
//! monotonic clock chosen by the caller) so every rule is unit-testable.

use crate::protocol::{GamePhase, PlayerInfo, PublicQuestion, RankEntry, ScoreEntry};
use quiz_core::Question;
use std::collections::HashMap;

use std::time::Duration;
use thiserror::Error;

pub const BASE_POINTS: u32 = 500;
pub const MAX_BONUS: u32 = 500;
// Score = BASE_POINTS + bonus per correct answer (max 1000). With a time limit the
// bonus scales with remaining time; without one it depends on the rank among correct
// answers, using ORDER_BONUS.
const ORDER_BONUS: [u32; 5] = [500, 400, 300, 200, 100];

pub type PlayerId = u32;
pub const HOST_ID: PlayerId = 0;
pub const MAX_PLAYERS: usize = 8;
pub const MAX_NAME_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum JoinError {
    #[error("le nom est vide")]
    EmptyName,
    #[error("le nom est trop long (24 caractères maximum)")]
    NameTooLong,
    #[error("ce nom est déjà pris")]
    NameTaken,
    #[error("la partie est pleine (8 joueurs maximum)")]
    LobbyFull,
}

impl JoinError {
    pub fn message(self, lang: quiz_core::Lang) -> &'static str {
        crate::protocol::ErrorCode::from(self).message(lang)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GameError {
    #[error("la partie a déjà commencé")]
    AlreadyStarted,
    #[error("aucune question")]
    NoQuestions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Lobby,
    Question {
        index: usize,
        deadline_ms: Option<u64>,
    },
    Reveal {
        index: usize,
    },
    Ended,
}

#[derive(Debug, Clone)]
struct Player {
    id: PlayerId,
    name: String,
    connected: bool,
    score: u32,
}

pub fn speed_bonus(remaining_ms: u64, limit_ms: u64) -> u32 {
    let limit = limit_ms.max(1);
    let r = remaining_ms.min(limit);
    (f64::from(MAX_BONUS) * r as f64 / limit as f64).round() as u32
}

pub fn order_bonus(n: usize) -> u32 {
    ORDER_BONUS.get(n).copied().unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
struct Answer {
    choice: usize,
    remaining_ms: Option<u64>,
    seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealInfo {
    pub question_id: u32,
    pub correct: usize,
    pub explanation: String,
    pub scores: Vec<ScoreEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advance {
    Question(PublicQuestion),
    End(Vec<RankEntry>),
}

#[derive(Debug)]
pub struct Game {
    players: Vec<Player>,
    next_id: PlayerId,
    questions: Vec<Question>,
    phase: Phase,
    answers: HashMap<PlayerId, Answer>,
    time_limit_ms: Option<u64>,
}

/// Ties share a rank and the next rank is skipped (1, 1, 3).
pub fn rank(mut entries: Vec<ScoreEntry>) -> Vec<RankEntry> {
    entries.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    let scores: Vec<u32> = entries.iter().map(|e| e.score).collect();
    entries
        .into_iter()
        .map(|e| RankEntry {
            rank: 1 + scores.iter().filter(|s| **s > e.score).count() as u32,
            player_id: e.player_id,
            name: e.name,
            score: e.score,
        })
        .collect()
}

impl Game {
    pub fn new(time_limit: Option<Duration>) -> Self {
        Self {
            players: Vec::new(),
            next_id: 0,
            questions: Vec::new(),
            phase: Phase::Lobby,
            answers: HashMap::new(),
            time_limit_ms: time_limit.map(|d| d.as_millis() as u64),
        }
    }

    /// Joining is allowed in any phase. Names must be unique among *connected* players, so
    /// a disconnected player can come back under the same name (with 0 points). A late
    /// joiner is expected to answer the current question as soon as they arrive.
    pub fn add_player(&mut self, name: &str) -> Result<PlayerId, JoinError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(JoinError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(JoinError::NameTooLong);
        }
        if self.players.iter().filter(|p| p.connected).count() >= MAX_PLAYERS {
            return Err(JoinError::LobbyFull);
        }
        if self
            .players
            .iter()
            .any(|p| p.connected && p.name.to_lowercase() == name.to_lowercase())
        {
            return Err(JoinError::NameTaken);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.players.push(Player {
            id,
            name: name.to_string(),
            connected: true,
            score: 0,
        });
        Ok(id)
    }

    /// In the lobby the player is removed; during a game they stay (score kept) but are no
    /// longer waited for.
    pub fn remove_player(&mut self, id: PlayerId) -> bool {
        if self.phase == Phase::Lobby {
            let before = self.players.len();
            self.players.retain(|p| p.id != id);
            return self.players.len() != before;
        }
        match self.players.iter_mut().find(|p| p.id == id && p.connected) {
            Some(p) => {
                p.connected = false;
                true
            }
            None => false,
        }
    }

    pub fn lobby(&self) -> Vec<PlayerInfo> {
        self.players
            .iter()
            .filter(|p| p.connected)
            .map(|p| PlayerInfo {
                id: p.id,
                name: p.name.clone(),
                is_host: p.id == HOST_ID,
            })
            .collect()
    }

    fn public(&self, index: usize, now_ms: u64) -> PublicQuestion {
        let remaining = match self.phase {
            Phase::Question { deadline_ms, .. } => deadline_ms.map(|d| d.saturating_sub(now_ms)),
            _ => None,
        };
        PublicQuestion::from_question(
            &self.questions[index],
            index,
            self.questions.len(),
            self.time_limit_ms.map(|ms| ms.div_ceil(1000) as u32),
            remaining,
        )
    }

    pub fn current_public(&self, now_ms: u64) -> Option<PublicQuestion> {
        match self.phase {
            Phase::Question { index, .. } => Some(self.public(index, now_ms)),
            _ => None,
        }
    }

    pub fn phase(&self) -> GamePhase {
        match self.phase {
            Phase::Lobby => GamePhase::Lobby,
            Phase::Question { .. } => GamePhase::Question,
            Phase::Reveal { .. } => GamePhase::Reveal,
            Phase::Ended => GamePhase::Ended,
        }
    }

    pub fn set_time_limit(&mut self, limit: Option<Duration>) -> bool {
        if self.phase != Phase::Lobby {
            return false;
        }
        self.time_limit_ms = limit.map(|d| d.as_millis() as u64);
        true
    }

    pub fn is_ended(&self) -> bool {
        self.phase == Phase::Ended
    }

    /// Only valid after the game ended. Disconnected players are purged, scores reset.
    pub fn back_to_lobby(&mut self) -> bool {
        if self.phase != Phase::Ended {
            return false;
        }
        self.players.retain(|p| p.connected);
        for p in &mut self.players {
            p.score = 0;
        }
        self.questions.clear();
        self.answers.clear();
        self.phase = Phase::Lobby;
        true
    }

    pub fn start(
        &mut self,
        questions: Vec<Question>,
        now_ms: u64,
    ) -> Result<PublicQuestion, GameError> {
        if self.phase != Phase::Lobby {
            return Err(GameError::AlreadyStarted);
        }
        if questions.is_empty() {
            return Err(GameError::NoQuestions);
        }
        self.questions = questions;
        self.begin_question(0, now_ms);
        Ok(self.public(0, now_ms))
    }

    fn begin_question(&mut self, index: usize, now_ms: u64) {
        self.answers.clear();
        self.phase = Phase::Question {
            index,
            deadline_ms: self.time_limit_ms.map(|l| now_ms + l),
        };
    }

    pub fn in_lobby(&self) -> bool {
        self.phase == Phase::Lobby
    }

    pub fn in_question(&self) -> bool {
        matches!(self.phase, Phase::Question { .. })
    }

    pub fn in_reveal(&self) -> bool {
        matches!(self.phase, Phase::Reveal { .. })
    }

    pub fn deadline_ms(&self) -> Option<u64> {
        match self.phase {
            Phase::Question { deadline_ms, .. } => deadline_ms,
            _ => None,
        }
    }

    pub fn current_question_id(&self) -> Option<u32> {
        match self.phase {
            Phase::Question { index, .. } => Some(self.questions[index].id),
            _ => None,
        }
    }

    /// Returns `false` (answer ignored) when out of phase, for a stale `question_id`, after
    /// the deadline, for an unknown/disconnected player, for a repeat, or for a bad choice.
    pub fn answer(&mut self, id: PlayerId, question_id: u32, choice: usize, now_ms: u64) -> bool {
        let Phase::Question { index, deadline_ms } = self.phase else {
            return false;
        };
        let q = &self.questions[index];
        if q.id != question_id
            || deadline_ms.is_some_and(|d| now_ms >= d)
            || choice >= q.choices.len()
        {
            return false;
        }
        if !self.players.iter().any(|p| p.id == id && p.connected) {
            return false;
        }
        if self.answers.contains_key(&id) {
            return false;
        }
        // Arrival order decides the unlimited-time bonus.
        let seq = self.answers.len() as u32;
        self.answers.insert(
            id,
            Answer {
                choice,
                remaining_ms: deadline_ms.map(|d| d - now_ms),
                seq,
            },
        );
        true
    }

    pub fn answer_progress(&self) -> (usize, usize) {
        let connected = self.players.iter().filter(|p| p.connected);
        let expected = connected.clone().count();
        let answered = connected
            .filter(|p| self.answers.contains_key(&p.id))
            .count();
        (answered, expected)
    }

    pub fn all_answered(&self) -> bool {
        let (a, e) = self.answer_progress();
        a == e
    }

    /// Scoring happens here, not on `answer`, so ranks among correct answers are final.
    pub fn reveal(&mut self) -> Option<RevealInfo> {
        let Phase::Question { index, .. } = self.phase else {
            return None;
        };
        let q = &self.questions[index];
        let mut correct: Vec<(PlayerId, Answer)> = self
            .answers
            .iter()
            .filter(|(_, a)| a.choice == q.correct)
            .map(|(id, a)| (*id, *a))
            .collect();
        correct.sort_by_key(|(_, a)| a.seq);
        let mut gained: HashMap<PlayerId, u32> = HashMap::new();
        for (n, (id, a)) in correct.iter().enumerate() {
            let bonus = match (a.remaining_ms, self.time_limit_ms) {
                (Some(rem), Some(limit)) => speed_bonus(rem, limit),
                _ => order_bonus(n),
            };
            gained.insert(*id, BASE_POINTS + bonus);
        }
        for p in &mut self.players {
            p.score += gained.get(&p.id).copied().unwrap_or(0);
        }
        let scores = self
            .players
            .iter()
            .map(|p| ScoreEntry {
                player_id: p.id,
                name: p.name.clone(),
                score: p.score,
                gained: gained.get(&p.id).copied().unwrap_or(0),
                choice: self.answers.get(&p.id).map(|a| a.choice),
            })
            .collect();
        let info = RevealInfo {
            question_id: q.id,
            correct: q.correct,
            explanation: q.explanation.clone(),
            scores,
        };
        self.phase = Phase::Reveal { index };
        Some(info)
    }

    pub fn advance(&mut self, now_ms: u64) -> Option<Advance> {
        let Phase::Reveal { index } = self.phase else {
            return None;
        };
        if index + 1 < self.questions.len() {
            self.begin_question(index + 1, now_ms);
            Some(Advance::Question(self.public(index + 1, now_ms)))
        } else {
            self.phase = Phase::Ended;
            Some(Advance::End(self.ranking()))
        }
    }

    pub fn scores(&self) -> Vec<ScoreEntry> {
        self.players
            .iter()
            .map(|p| ScoreEntry {
                player_id: p.id,
                name: p.name.clone(),
                score: p.score,
                gained: 0,
                choice: None,
            })
            .collect()
    }

    pub fn ranking(&self) -> Vec<RankEntry> {
        rank(self.scores())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qs(n: u32) -> Vec<Question> {
        (0..n)
            .map(|id| Question {
                id,
                prompt: format!("Q{id}"),
                choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                correct: 1,
                explanation: "e".into(),
            })
            .collect()
    }

    fn game_with(limit: Option<Duration>) -> Game {
        let mut g = Game::new(limit);
        assert_eq!(g.add_player("Hôte").unwrap(), HOST_ID);
        g.add_player("Alice").unwrap();
        g.add_player("Bob").unwrap();
        g
    }

    fn game3() -> Game {
        game_with(Some(Duration::from_secs(10)))
    }

    fn unlimited() -> Game {
        game_with(None)
    }

    #[test]
    fn join_rules() {
        let mut g = Game::new(None);
        assert_eq!(g.add_player("  "), Err(JoinError::EmptyName));
        assert_eq!(g.add_player(&"x".repeat(25)), Err(JoinError::NameTooLong));
        g.add_player("Alice").unwrap();
        assert_eq!(g.add_player(" alice "), Err(JoinError::NameTaken));
        for i in 1..MAX_PLAYERS {
            g.add_player(&format!("p{i}")).unwrap();
        }
        assert_eq!(g.add_player("extra"), Err(JoinError::LobbyFull));
    }

    #[test]
    fn join_allowed_during_game_with_zero_points() {
        let mut g = game3();
        g.start(qs(2), 0).unwrap();
        let id = g.add_player("Late").unwrap();
        assert_eq!(g.answer_progress(), (0, 4), "attendu dès son arrivée");
        assert!(g.answer(id, 0, 1, 10));
        g.reveal().unwrap();
        let id2 = g.add_player("Later").unwrap();
        assert_eq!(
            g.scores()
                .iter()
                .find(|s| s.player_id == id2)
                .unwrap()
                .score,
            0
        );
        g.advance(100).unwrap();
        assert_eq!(g.answer_progress(), (0, 5));
    }

    #[test]
    fn join_allowed_after_end_and_name_reusable_after_disconnect() {
        let mut g = game3();
        g.start(qs(1), 0).unwrap();
        g.reveal().unwrap();
        g.advance(0).unwrap();
        assert!(g.is_ended());
        g.add_player("Late").unwrap();
        g.remove_player(1);
        g.add_player("Alice").unwrap();
    }

    #[test]
    fn start_rules() {
        let mut g = game3();
        assert_eq!(g.start(vec![], 0), Err(GameError::NoQuestions));
        let pq = g.start(qs(3), 0).unwrap();
        assert_eq!((pq.index, pq.total), (0, 3));
        assert_eq!(pq.time_limit_secs, Some(10));
        assert_eq!(pq.remaining_ms, Some(10_000));
        assert_eq!(g.start(qs(3), 0), Err(GameError::AlreadyStarted));
    }

    #[test]
    fn unlimited_has_no_deadline_and_no_limit_fields() {
        let mut g = unlimited();
        let pq = g.start(qs(1), 0).unwrap();
        assert_eq!((pq.time_limit_secs, pq.remaining_ms), (None, None));
        assert_eq!(g.deadline_ms(), None);
        assert!(g.answer(0, 0, 1, 10_000_000), "jamais en retard");
        assert!(g.in_question());
    }

    #[test]
    fn current_public_gives_remaining_time() {
        let mut g = game3();
        assert!(g.current_public(0).is_none());
        g.start(qs(2), 1000).unwrap();
        assert_eq!(g.current_public(4000).unwrap().remaining_ms, Some(7000));
        assert_eq!(g.current_public(99_000).unwrap().remaining_ms, Some(0));
    }

    #[test]
    fn remove_in_lobby_and_in_game() {
        let mut g = game3();
        assert!(g.remove_player(2));
        assert!(!g.remove_player(2));
        assert_eq!(g.lobby().len(), 2);
        g.start(qs(2), 0).unwrap();
        assert!(g.remove_player(1));
        assert!(!g.remove_player(1));
        assert_eq!(g.lobby().len(), 1);
        assert_eq!(g.scores().len(), 2, "garde son score");
    }

    #[test]
    fn answer_rules() {
        let mut g = game3();
        g.start(qs(2), 1000).unwrap();
        assert_eq!(g.deadline_ms(), Some(11_000));
        assert!(!g.answer(1, 99, 1, 1500), "mauvaise question_id");
        assert!(!g.answer(1, 0, 4, 1500), "choix hors bornes");
        assert!(!g.answer(42, 0, 1, 1500), "joueur inconnu");
        assert!(!g.answer(1, 0, 1, 11_000), "après le délai");
        assert!(g.answer(1, 0, 1, 10_999));
        assert!(!g.answer(1, 0, 2, 2000), "une seule réponse");
        assert_eq!(g.answer_progress(), (1, 3));
    }

    #[test]
    fn answer_outside_question_phase_ignored() {
        let mut g = game3();
        assert!(!g.answer(1, 0, 1, 0));
        g.start(qs(1), 0).unwrap();
        g.reveal().unwrap();
        assert!(!g.answer(1, 0, 1, 1));
    }

    #[test]
    fn all_answered_ignores_disconnected() {
        let mut g = game3();
        g.start(qs(1), 0).unwrap();
        assert!(!g.all_answered());
        g.answer(0, 0, 1, 1);
        g.answer(1, 0, 0, 1);
        assert!(!g.all_answered());
        g.remove_player(2);
        assert!(g.all_answered());
    }

    #[test]
    fn speed_bonus_formula() {
        assert_eq!(speed_bonus(10_000, 10_000), 500);
        assert_eq!(speed_bonus(5_000, 10_000), 250);
        assert_eq!(speed_bonus(0, 10_000), 0);
        assert_eq!(speed_bonus(1, 3), 167, "arrondi");
        assert_eq!(speed_bonus(99, 10), 500, "plafonné");
        assert_eq!(speed_bonus(5, 0), 500, "limite nulle sans panique");
    }

    #[test]
    fn order_bonus_table() {
        let v: Vec<u32> = (0..7).map(order_bonus).collect();
        assert_eq!(v, [500, 400, 300, 200, 100, 0, 0]);
    }

    #[test]
    fn limited_scoring_uses_remaining_time() {
        let mut g = game3();
        g.start(qs(1), 0).unwrap();
        g.answer(0, 0, 1, 0);
        g.answer(1, 0, 1, 5_000);
        // Wrong answers never score and do not take a rank.
        g.answer(2, 0, 3, 100);
        let r = g.reveal().unwrap();
        let gained: Vec<u32> = r.scores.iter().map(|e| e.gained).collect();
        assert_eq!(gained, [1000, 750, 0]);
        let totals: Vec<u32> = r.scores.iter().map(|e| e.score).collect();
        assert_eq!(totals, [1000, 750, 0]);
        let choices: Vec<Option<usize>> = r.scores.iter().map(|e| e.choice).collect();
        assert_eq!(choices, [Some(1), Some(1), Some(3)]);
    }

    #[test]
    fn limited_last_instant_gets_base_only() {
        let mut g = game3();
        g.start(qs(1), 0).unwrap();
        g.answer(0, 0, 1, 9_999);
        let r = g.reveal().unwrap();
        assert_eq!(r.scores[0].gained, 500);
    }

    #[test]
    fn unlimited_scoring_by_order_of_correct_answers() {
        let mut g = unlimited();
        for i in 3..8 {
            g.add_player(&format!("p{i}")).unwrap();
        }
        g.start(qs(1), 0).unwrap();
        // A wrong answer does not consume a bonus rank.
        g.answer(3, 0, 0, 1);
        g.answer(0, 0, 1, 2);
        g.answer(1, 0, 1, 3);
        g.answer(2, 0, 1, 4);
        g.answer(4, 0, 1, 5);
        g.answer(5, 0, 1, 6);
        g.answer(6, 0, 1, 7);
        let r = g.reveal().unwrap();
        let gained: Vec<u32> = r.scores.iter().map(|e| e.gained).collect();
        assert_eq!(gained, [1000, 900, 800, 0, 700, 600, 500, 0]);
    }

    #[test]
    fn no_answer_scores_zero_and_totals_accumulate() {
        let mut g = unlimited();
        g.start(qs(2), 0).unwrap();
        g.answer(0, 0, 1, 1);
        let r = g.reveal().unwrap();
        assert_eq!(r.scores[1].gained, 0);
        assert_eq!(r.scores[1].choice, None);
        g.advance(2).unwrap();
        g.answer(0, 1, 1, 3);
        let r = g.reveal().unwrap();
        assert_eq!(r.scores[0].score, 2000);
    }

    #[test]
    fn force_reveal_without_all_answers() {
        let mut g = unlimited();
        g.start(qs(1), 0).unwrap();
        g.answer(1, 0, 1, 1);
        assert!(!g.all_answered());
        assert!(g.reveal().is_some(), "révélation forcée");
        assert!(g.in_reveal());
        assert!(g.reveal().is_none(), "pas de double révélation");
    }

    #[test]
    fn disconnected_player_keeps_answer_and_score() {
        let mut g = game3();
        g.start(qs(2), 0).unwrap();
        g.answer(1, 0, 1, 1);
        g.remove_player(1);
        let r = g.reveal().unwrap();
        assert!(r.scores[1].score >= 500);
    }

    #[test]
    fn advance_and_end() {
        let mut g = unlimited();
        assert!(g.advance(0).is_none());
        g.start(qs(2), 0).unwrap();
        assert!(g.advance(0).is_none(), "pas avant la révélation");
        g.answer(0, 0, 1, 1);
        g.reveal().unwrap();
        let Some(Advance::Question(q)) = g.advance(5000) else {
            panic!("question attendue");
        };
        assert_eq!(q.index, 1);
        g.answer(0, 1, 1, 5001);
        g.answer(1, 1, 1, 5002);
        g.reveal().unwrap();
        let Some(Advance::End(r)) = g.advance(6000) else {
            panic!("fin attendue");
        };
        assert_eq!(r[0].name, "Hôte");
        assert_eq!((r[0].rank, r[0].score), (1, 2000));
        assert_eq!((r[1].rank, r[1].score), (2, 900));
        assert_eq!((r[2].rank, r[2].score), (3, 0));
        assert!(g.advance(7000).is_none());
    }

    #[test]
    fn back_to_lobby_resets_scores_keeps_players() {
        let mut g = unlimited();
        assert!(!g.back_to_lobby(), "seulement après la fin");
        g.start(qs(1), 0).unwrap();
        g.answer(0, 0, 1, 1);
        g.reveal().unwrap();
        assert!(!g.back_to_lobby());
        g.advance(2).unwrap();
        g.remove_player(2);
        assert!(g.back_to_lobby());
        assert!(g.in_lobby());
        assert_eq!(g.scores().len(), 2, "déconnecté purgé");
        assert!(g.scores().iter().all(|s| s.score == 0));
        g.start(qs(1), 10).unwrap();
        assert_eq!(g.start(qs(1), 10), Err(GameError::AlreadyStarted));
    }

    #[test]
    fn set_time_limit_only_in_lobby() {
        let mut g = unlimited();
        assert_eq!(g.phase(), GamePhase::Lobby);
        assert!(g.set_time_limit(Some(Duration::from_secs(7))));
        assert_eq!(g.start(qs(1), 0).unwrap().time_limit_secs, Some(7));
        assert_eq!(g.phase(), GamePhase::Question);
        assert!(!g.set_time_limit(None));
        g.reveal().unwrap();
        assert_eq!(g.phase(), GamePhase::Reveal);
        g.advance(0).unwrap();
        assert_eq!(g.phase(), GamePhase::Ended);
        assert!(g.back_to_lobby());
        assert!(g.set_time_limit(None));
        assert_eq!(g.start(qs(1), 0).unwrap().time_limit_secs, None);
    }

    #[test]
    fn ranking_ties() {
        let e = |id, name: &str, score| ScoreEntry {
            player_id: id,
            name: name.into(),
            score,
            gained: 0,
            choice: None,
        };
        let r = rank(vec![
            e(0, "d", 1),
            e(1, "b", 3),
            e(2, "a", 3),
            e(3, "c", 2),
            e(4, "e", 1),
        ]);
        let v: Vec<(u32, &str)> = r.iter().map(|x| (x.rank, x.name.as_str())).collect();
        assert_eq!(v, [(1, "a"), (1, "b"), (3, "c"), (4, "d"), (4, "e")]);
    }

    #[test]
    fn time_limit_secs_rounds_up() {
        let mut g = Game::new(Some(Duration::from_millis(300)));
        g.add_player("a").unwrap();
        assert_eq!(g.start(qs(1), 0).unwrap().time_limit_secs, Some(1));
    }
}

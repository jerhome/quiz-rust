//! Pure domain: no I/O and no async, so the state machine is trivially testable.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CHOICES_PER_QUESTION: usize = 4;
pub const COUNT_RANGE: std::ops::RangeInclusive<u8> = 5..=20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: u32,
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct: usize,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Facile,
    Moyen,
    Difficile,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Facile, Difficulty::Moyen, Difficulty::Difficile];

    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Difficulty::Facile, Lang::Fr) => "Facile",
            (Difficulty::Moyen, Lang::Fr) => "Moyen",
            (Difficulty::Difficile, Lang::Fr) => "Difficile",
            (Difficulty::Facile, Lang::En) => "Easy",
            (Difficulty::Moyen, Lang::En) => "Medium",
            (Difficulty::Difficile, Lang::En) => "Hard",
        }
    }

    pub fn label_fr(self) -> &'static str {
        self.label(Lang::Fr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizConfig {
    pub theme: String,
    pub difficulty: Difficulty,
    pub count: u8,
    pub language: Lang,
}

impl Default for QuizConfig {
    fn default() -> Self {
        Self {
            theme: String::new(),
            difficulty: Difficulty::Moyen,
            count: 5,
            language: Lang::Fr,
        }
    }
}

impl QuizConfig {
    pub fn validate(&self) -> Result<(), QuizError> {
        if self.theme.trim().is_empty() {
            return Err(QuizError::InvalidConfig(ConfigIssue::EmptyTheme));
        }
        if !COUNT_RANGE.contains(&self.count) {
            return Err(QuizError::InvalidConfig(ConfigIssue::BadCount));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuizState {
    Setup,
    Generating,
    Question { index: usize },
    Reveal { index: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigIssue {
    EmptyTheme,
    BadCount,
}

impl ConfigIssue {
    fn message(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (ConfigIssue::EmptyTheme, Lang::Fr) => "le thème est vide",
            (ConfigIssue::EmptyTheme, Lang::En) => "the theme is empty",
            (ConfigIssue::BadCount, Lang::Fr) => "le nombre de questions doit être entre 5 et 20",
            (ConfigIssue::BadCount, Lang::En) => "the number of questions must be between 5 and 20",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuizError {
    #[error("action « {action} » impossible dans l'état {state:?}")]
    InvalidTransition {
        action: &'static str,
        state: QuizState,
    },
    #[error("configuration invalide : {}", .0.message(Lang::Fr))]
    InvalidConfig(ConfigIssue),
    #[error("choix {0} hors bornes")]
    InvalidChoice(usize),
    #[error("aucune question fournie")]
    NoQuestions,
}

impl QuizError {
    pub fn message(&self, lang: Lang) -> String {
        match (self, lang) {
            (QuizError::InvalidConfig(_), Lang::Fr) => self.to_string(),
            (QuizError::InvalidConfig(i), Lang::En) => {
                format!("invalid configuration: {}", i.message(Lang::En))
            }
            (QuizError::InvalidTransition { action, state }, Lang::En) => {
                format!("action \"{action}\" not allowed in state {state:?}")
            }
            (QuizError::InvalidChoice(c), Lang::En) => format!("choice {c} out of range"),
            (QuizError::NoQuestions, Lang::En) => "no questions provided".to_string(),
            (_, Lang::Fr) => self.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Quiz {
    config: QuizConfig,
    state: QuizState,
    questions: Vec<Question>,
    answers: Vec<Option<usize>>,
    score: usize,
}

impl Quiz {
    pub fn new(config: QuizConfig) -> Self {
        Self {
            config,
            state: QuizState::Setup,
            questions: Vec::new(),
            answers: Vec::new(),
            score: 0,
        }
    }

    fn invalid<T>(&self, action: &'static str) -> Result<T, QuizError> {
        Err(QuizError::InvalidTransition {
            action,
            state: self.state,
        })
    }

    pub fn set_config(&mut self, config: QuizConfig) -> Result<(), QuizError> {
        if self.state != QuizState::Setup {
            return self.invalid("set_config");
        }
        self.config = config;
        Ok(())
    }

    pub fn start_generation(&mut self) -> Result<(), QuizError> {
        if self.state != QuizState::Setup {
            return self.invalid("start_generation");
        }
        self.config.validate()?;
        self.state = QuizState::Generating;
        Ok(())
    }

    pub fn questions_ready(&mut self, questions: Vec<Question>) -> Result<(), QuizError> {
        if self.state != QuizState::Generating {
            return self.invalid("questions_ready");
        }
        if questions.is_empty() {
            return Err(QuizError::NoQuestions);
        }
        self.answers = vec![None; questions.len()];
        self.questions = questions;
        self.score = 0;
        self.state = QuizState::Question { index: 0 };
        Ok(())
    }

    /// Back to Setup with the config kept, so the UI can retry via `start_generation()`.
    pub fn generation_failed(&mut self) -> Result<(), QuizError> {
        if self.state != QuizState::Generating {
            return self.invalid("generation_failed");
        }
        self.state = QuizState::Setup;
        Ok(())
    }

    /// An out-of-range choice is rejected and leaves the state unchanged.
    pub fn answer(&mut self, choice: usize) -> Result<bool, QuizError> {
        let QuizState::Question { index } = self.state else {
            return self.invalid("answer");
        };
        let q = &self.questions[index];
        if choice >= q.choices.len() {
            return Err(QuizError::InvalidChoice(choice));
        }
        let ok = choice == q.correct;
        self.answers[index] = Some(choice);
        if ok {
            self.score += 1;
        }
        self.state = QuizState::Reveal { index };
        Ok(ok)
    }

    // This is not an iterator; the name mirrors the Reveal -> Question action.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<(), QuizError> {
        let QuizState::Reveal { index } = self.state else {
            return self.invalid("next");
        };
        self.state = if index + 1 < self.questions.len() {
            QuizState::Question { index: index + 1 }
        } else {
            QuizState::Finished
        };
        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), QuizError> {
        if self.state != QuizState::Finished {
            return self.invalid("restart");
        }
        self.questions.clear();
        self.answers.clear();
        self.score = 0;
        self.state = QuizState::Setup;
        Ok(())
    }

    pub fn state(&self) -> QuizState {
        self.state
    }

    pub fn current_question(&self) -> Option<&Question> {
        match self.state {
            QuizState::Question { index } | QuizState::Reveal { index } => {
                self.questions.get(index)
            }
            _ => None,
        }
    }

    pub fn selected_answer(&self, index: usize) -> Option<usize> {
        self.answers.get(index).copied().flatten()
    }

    pub fn score(&self) -> usize {
        self.score
    }

    pub fn total(&self) -> usize {
        self.questions.len()
    }

    pub fn config(&self) -> &QuizConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> QuizConfig {
        QuizConfig {
            theme: "Histoire".into(),
            ..QuizConfig::default()
        }
    }

    fn qs(n: usize) -> Vec<Question> {
        (0..n)
            .map(|i| Question {
                id: i as u32,
                prompt: format!("Q{i}"),
                choices: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                correct: 1,
                explanation: "e".into(),
            })
            .collect()
    }

    fn generating() -> Quiz {
        let mut q = Quiz::new(cfg());
        q.start_generation().unwrap();
        q
    }

    fn in_question(n: usize) -> Quiz {
        let mut q = generating();
        q.questions_ready(qs(n)).unwrap();
        q
    }

    fn is_invalid<T: std::fmt::Debug>(r: Result<T, QuizError>) -> bool {
        matches!(r, Err(QuizError::InvalidTransition { .. }))
    }

    #[test]
    fn new_is_setup() {
        let q = Quiz::new(cfg());
        assert_eq!(q.state(), QuizState::Setup);
        assert_eq!(q.total(), 0);
        assert_eq!(q.score(), 0);
        assert!(q.current_question().is_none());
    }

    #[test]
    fn config_validation() {
        assert!(cfg().validate().is_ok());
        assert!(QuizConfig::default().validate().is_err());
        for c in [4, 21, 0] {
            let k = QuizConfig { count: c, ..cfg() };
            assert!(k.validate().is_err());
        }
        for c in [5, 20] {
            assert!(QuizConfig { count: c, ..cfg() }.validate().is_ok());
        }
        assert_eq!(QuizConfig::default().count, 5);
    }

    #[test]
    fn lang_serde_and_names() {
        assert_eq!(serde_json::to_string(&Lang::Fr).unwrap(), "\"fr\"");
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"en\"").unwrap(), Lang::En);
        assert!(serde_json::from_str::<Lang>("\"de\"").is_err());
        assert_eq!(Lang::default(), Lang::Fr);
        assert_eq!(Lang::ALL, [Lang::Fr, Lang::En]);
        assert_eq!(Lang::Fr.native_name(), "Français");
        assert_eq!(Lang::En.native_name(), "English");
    }

    #[test]
    fn english_difficulty_labels() {
        let l: Vec<_> = Difficulty::ALL.iter().map(|d| d.label(Lang::En)).collect();
        assert_eq!(l, ["Easy", "Medium", "Hard"]);
    }

    #[test]
    fn error_messages_both_languages() {
        let errs = [
            QuizError::InvalidTransition {
                action: "next",
                state: QuizState::Setup,
            },
            QuizError::InvalidConfig(ConfigIssue::EmptyTheme),
            QuizError::InvalidConfig(ConfigIssue::BadCount),
            QuizError::InvalidChoice(9),
            QuizError::NoQuestions,
        ];
        for e in &errs {
            let (fr, en) = (e.message(Lang::Fr), e.message(Lang::En));
            assert!(!fr.is_empty() && !en.is_empty());
            assert_ne!(fr, en, "{e:?}");
        }
    }

    #[test]
    fn difficulty_labels() {
        let l: Vec<_> = Difficulty::ALL.iter().map(|d| d.label_fr()).collect();
        assert_eq!(l, ["Facile", "Moyen", "Difficile"]);
    }

    #[test]
    fn start_generation_ok_and_invalid() {
        let mut q = Quiz::new(cfg());
        q.start_generation().unwrap();
        assert_eq!(q.state(), QuizState::Generating);
        assert!(is_invalid(q.start_generation()));
    }

    #[test]
    fn start_generation_rejects_bad_config() {
        let mut q = Quiz::new(QuizConfig::default());
        assert!(matches!(
            q.start_generation(),
            Err(QuizError::InvalidConfig(_))
        ));
        assert_eq!(q.state(), QuizState::Setup);
    }

    #[test]
    fn set_config_only_in_setup() {
        let mut q = Quiz::new(cfg());
        q.set_config(QuizConfig { count: 10, ..cfg() }).unwrap();
        assert_eq!(q.config().count, 10);
        q.start_generation().unwrap();
        assert!(is_invalid(q.set_config(cfg())));
    }

    #[test]
    fn questions_ready_ok() {
        let q = in_question(5);
        assert_eq!(q.state(), QuizState::Question { index: 0 });
        assert_eq!(q.total(), 5);
        assert_eq!(q.current_question().unwrap().id, 0);
    }

    #[test]
    fn questions_ready_invalid_and_empty() {
        let mut q = Quiz::new(cfg());
        assert!(is_invalid(q.questions_ready(qs(5))));
        let mut q = generating();
        assert_eq!(q.questions_ready(vec![]), Err(QuizError::NoQuestions));
        assert_eq!(q.state(), QuizState::Generating);
    }

    #[test]
    fn generation_failed_allows_retry() {
        let mut q = generating();
        q.generation_failed().unwrap();
        assert_eq!(q.state(), QuizState::Setup);
        assert_eq!(q.config().theme, "Histoire");
        q.start_generation().unwrap();
        assert_eq!(q.state(), QuizState::Generating);
        let mut s = Quiz::new(cfg());
        assert!(is_invalid(s.generation_failed()));
    }

    #[test]
    fn answer_correct_and_wrong() {
        let mut q = in_question(5);
        assert_eq!(q.answer(1), Ok(true));
        assert_eq!(q.state(), QuizState::Reveal { index: 0 });
        assert_eq!(q.score(), 1);
        assert_eq!(q.selected_answer(0), Some(1));
        q.next().unwrap();
        assert_eq!(q.answer(3), Ok(false));
        assert_eq!(q.score(), 1);
        assert_eq!(q.selected_answer(1), Some(3));
        assert_eq!(q.selected_answer(2), None);
    }

    #[test]
    fn answer_invalid() {
        let mut q = in_question(5);
        assert_eq!(q.answer(4), Err(QuizError::InvalidChoice(4)));
        assert_eq!(q.state(), QuizState::Question { index: 0 });
        q.answer(0).unwrap();
        assert!(is_invalid(q.answer(1)), "double réponse");
        assert!(is_invalid(Quiz::new(cfg()).answer(0)));
    }

    #[test]
    fn next_advances_and_finishes() {
        let mut q = in_question(5);
        q.answer(0).unwrap();
        q.next().unwrap();
        assert_eq!(q.state(), QuizState::Question { index: 1 });
        for _ in 1..5 {
            q.answer(1).unwrap();
            q.next().unwrap();
        }
        assert_eq!(q.state(), QuizState::Finished);
        assert!(q.current_question().is_none());
        assert_eq!(q.score(), 4);
        assert_eq!(q.total(), 5);
    }

    #[test]
    fn next_invalid() {
        assert!(is_invalid(in_question(5).next()));
        assert!(is_invalid(Quiz::new(cfg()).next()));
    }

    #[test]
    fn restart_resets() {
        let mut q = in_question(5);
        for _ in 0..5 {
            q.answer(1).unwrap();
            q.next().unwrap();
        }
        assert_eq!(q.score(), 5);
        q.restart().unwrap();
        assert_eq!(q.state(), QuizState::Setup);
        assert_eq!((q.score(), q.total()), (0, 0));
        assert_eq!(q.config().theme, "Histoire");
        assert!(is_invalid(q.restart()));
        assert!(is_invalid(in_question(5).restart()));
    }
}

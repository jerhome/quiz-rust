use eframe::egui;
use quiz_core::{QuizState, CHOICES_PER_QUESTION};

use crate::app::QuizApp;
use crate::theme;
use crate::widgets::{self, AnswerState};

const KEYS: [egui::Key; CHOICES_PER_QUESTION] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
];

pub fn choice_for_key(key: egui::Key) -> Option<usize> {
    KEYS.iter().position(|k| *k == key)
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let QuizState::Question { index } = app.quiz.state() else {
        return;
    };
    let Some(q) = app.quiz.current_question().cloned() else {
        return;
    };
    widgets::question_header(ui, app.lang(), index, app.quiz.total(), None);
    widgets::prompt_card(ui, &q.prompt);
    ui.add_space(theme::GAP);
    let mut picked = ui
        .input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } => choice_for_key(*key),
                _ => None,
            })
        })
        .filter(|c| *c < q.choices.len());
    let items: Vec<_> = q
        .choices
        .iter()
        .map(|c| (c.as_str(), AnswerState::Idle))
        .collect();
    if let Some(c) = widgets::answer_grid(ui, &items, true) {
        picked = Some(c);
    }
    if let Some(c) = picked {
        let _ = app.quiz.answer(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touches_vers_choix() {
        assert_eq!(choice_for_key(egui::Key::Num1), Some(0));
        assert_eq!(choice_for_key(egui::Key::Num4), Some(3));
        assert_eq!(choice_for_key(egui::Key::Num5), None);
        assert_eq!(choice_for_key(egui::Key::A), None);
    }
}

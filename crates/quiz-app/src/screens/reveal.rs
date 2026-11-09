use eframe::egui;
use quiz_core::{Lang, QuizState};

use crate::i18n::{t, Key};

use crate::app::QuizApp;
use crate::theme;
use crate::widgets::{self, AnswerState, ButtonKind};

pub fn reveal_state(i: usize, correct: usize, selected: Option<usize>) -> AnswerState {
    if i == correct {
        AnswerState::Correct
    } else if selected == Some(i) {
        AnswerState::Wrong
    } else {
        AnswerState::Dim
    }
}

pub fn explanation_card(ui: &mut egui::Ui, text: &str, lang: Lang) {
    widgets::card_styled(ui, theme::SURFACE_2, theme::BORDER, |ui| {
        ui.label(theme::semi(t(lang, Key::Explanation), theme::SIZE_BODY).color(theme::TEXT_DIM));
        ui.label(egui::RichText::new(text).size(theme::SIZE_BODY));
    });
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let QuizState::Reveal { index } = app.quiz.state() else {
        return;
    };
    let Some(q) = app.quiz.current_question().cloned() else {
        return;
    };
    let selected = app.quiz.selected_answer(index);
    let lang = app.lang();
    widgets::question_header(ui, lang, index, app.quiz.total(), None);
    widgets::prompt_card(ui, &q.prompt);
    ui.add_space(theme::GAP);
    let items: Vec<_> = q
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), reveal_state(i, q.correct, selected)))
        .collect();
    widgets::answer_grid(ui, &items, false);
    if selected == Some(q.correct) {
        widgets::badge(ui, t(lang, Key::GoodAnswer), theme::SUCCESS);
    } else {
        widgets::badge(ui, t(lang, Key::BadAnswer), theme::DANGER);
    }
    explanation_card(ui, &q.explanation, lang);
    ui.add_space(theme::GAP);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let clicked = widgets::button(ui, t(lang, Key::Next), ButtonKind::Primary, None).clicked();
    if clicked || enter {
        let _ = app.quiz.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etats_de_revelation() {
        assert_eq!(reveal_state(2, 2, Some(0)), AnswerState::Correct);
        assert_eq!(reveal_state(0, 2, Some(0)), AnswerState::Wrong);
        assert_eq!(reveal_state(1, 2, Some(0)), AnswerState::Dim);
        assert_eq!(reveal_state(2, 2, Some(2)), AnswerState::Correct);
    }
}

use eframe::egui;

use crate::app::QuizApp;
use crate::i18n::{t, Key};
use crate::theme;
use crate::widgets::{self, ButtonKind};

pub fn score_label(score: usize, total: usize) -> String {
    format!("{score} / {total}")
}

pub fn verdict(score: usize, total: usize) -> Option<Key> {
    if total == 0 {
        return None;
    }
    Some(match score * 100 / total {
        100 => Key::VerdictPerfect,
        75..=99 => Key::VerdictGreat,
        50..=74 => Key::VerdictGood,
        25..=49 => Key::VerdictOk,
        _ => Key::VerdictLow,
    })
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let (score, total) = (app.quiz.score(), app.quiz.total());
    ui.add_space(40.0);
    widgets::card(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(theme::dim(t(lang, Key::QuizDone)));
            ui.label(theme::semi(score_label(score, total), 72.0).color(theme::ACCENT_HOVER));
            if let Some(v) = verdict(score, total) {
                ui.label(theme::semi(t(lang, v), theme::SIZE_SUBTITLE));
            }
            ui.add_space(theme::GAP);
            widgets::progress_bar(ui, score as f32 / total.max(1) as f32, theme::SUCCESS);
        });
    });
    ui.add_space(theme::GAP);
    if widgets::button(ui, t(lang, Key::PlayAgain), ButtonKind::Primary, None).clicked() {
        let _ = app.quiz.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatage_score() {
        assert_eq!(score_label(3, 5), "3 / 5");
    }

    #[test]
    fn appreciation() {
        assert_eq!(verdict(5, 5), Some(Key::VerdictPerfect));
        assert_eq!(verdict(0, 5), Some(Key::VerdictLow));
        assert_eq!(verdict(0, 0), None);
    }
}

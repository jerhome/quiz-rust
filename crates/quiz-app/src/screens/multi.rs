use std::time::{Duration, Instant};

use eframe::egui;
use quiz_core::Lang;
use quiz_net::ScoreEntry;

use crate::app::QuizApp;
use crate::i18n::{t, tf, Key};
use crate::multi::{is_tied, Session};
use crate::screens::question::choice_for_key;
use crate::screens::reveal::{explanation_card, reveal_state};
use crate::theme;
use crate::widgets::{self, AnswerState, ButtonKind};

fn pressed_choice(ui: &egui::Ui) -> Option<usize> {
    ui.input(|i| {
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
}

pub fn show_question(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    let Some(q) = s.question.clone() else { return };
    let now = Instant::now();
    let secs = s
        .remaining_secs(now)
        .map(|t| tf(lang, Key::Seconds, &[&t.to_string()]));
    widgets::question_header(ui, lang, q.index, q.total, secs.as_deref());
    if let Some(f) = s.time_fraction(now) {
        // No time limit means no bar and no periodic repaint; otherwise repaint
        // smoothly only while the bar is visible.
        widgets::timer_bar(ui, f);
        ui.add_space(theme::GAP);
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
    widgets::prompt_card(ui, &q.prompt);
    ui.add_space(theme::GAP);
    let answered = s.my_choice.is_some();
    let mut picked = if answered {
        None
    } else {
        pressed_choice(ui).filter(|c| *c < q.choices.len())
    };
    let items: Vec<_> = q
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let st = match (s.my_choice, answered) {
                (Some(m), _) if m == i => AnswerState::Selected,
                (_, true) => AnswerState::Dim,
                _ => AnswerState::Idle,
            };
            (c.as_str(), st)
        })
        .collect();
    if let Some(c) = widgets::answer_grid(ui, &items, !answered) {
        picked = Some(c);
    }
    if let Some(c) = picked {
        s.answer(c);
    }
    if answered {
        widgets::badge(ui, t(lang, Key::AnswerSent), theme::SUCCESS);
    }
    if s.is_host() {
        if let Some((a, e)) = s.progress {
            ui.label(theme::dim(tf(
                lang,
                Key::AnswersReceived,
                &[&a.to_string(), &e.to_string()],
            )));
        }
        ui.add_space(4.0);
        if widgets::button(ui, t(lang, Key::RevealNow), ButtonKind::Secondary, None).clicked() {
            s.force_reveal();
        }
    }
}

pub fn show_reveal(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    let Some(r) = s.reveal.clone() else { return };
    let Some(q) = s.question.clone() else {
        // Joined mid-reveal: no question is known, so show only the explanation and scores.
        widgets::heading(ui, t(lang, Key::RevealInProgress), None);
        explanation_card(ui, &r.explanation, lang);
        ui.add_space(theme::GAP);
        score_table(ui, &r.scores, s.my_id, lang);
        ui.add_space(theme::GAP);
        next_controls(ui, s, lang);
        return;
    };
    widgets::question_header(ui, lang, q.index, q.total, None);
    widgets::prompt_card(ui, &q.prompt);
    ui.add_space(theme::GAP);
    let mine = my_choice(&r.scores, s.my_id).or(s.my_choice);
    let items: Vec<_> = q
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), reveal_state(i, r.correct, mine)))
        .collect();
    widgets::answer_grid(ui, &items, false);
    explanation_card(ui, &r.explanation, lang);
    ui.add_space(theme::GAP);
    score_table(ui, &r.scores, s.my_id, lang);
    ui.add_space(theme::GAP);
    next_controls(ui, s, lang);
}

fn my_choice(scores: &[ScoreEntry], my_id: u32) -> Option<usize> {
    scores.iter().find(|e| e.player_id == my_id)?.choice
}

fn next_controls(ui: &mut egui::Ui, s: &mut Session, lang: Lang) {
    if !s.is_host() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
            ui.label(theme::dim(t(lang, Key::WaitingHost)));
        });
        return;
    }
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if widgets::button(ui, t(lang, Key::Next), ButtonKind::Primary, None).clicked() || enter {
        s.next_question();
    }
}

fn score_table(ui: &mut egui::Ui, scores: &[ScoreEntry], my_id: u32, lang: Lang) {
    widgets::card(ui, |ui| {
        ui.label(theme::semi(t(lang, Key::Scores), theme::SIZE_SUBTITLE));
        for e in scores {
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                let me = e.player_id == my_id;
                let color = if me {
                    theme::ACCENT_HOVER
                } else {
                    theme::TEXT_DIM
                };
                widgets::avatar(ui, &e.name, color);
                ui.label(egui::RichText::new(&e.name).size(theme::SIZE_BODY));
                if me {
                    widgets::badge(ui, t(lang, Key::You), theme::ACCENT_HOVER);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::semi(e.score.to_string(), theme::SIZE_SUBTITLE));
                    let gain = theme::semi(gain_label(e.gained), theme::SIZE_BODY);
                    let gain_color = if e.gained > 0 {
                        theme::SUCCESS
                    } else {
                        theme::TEXT_DIM
                    };
                    ui.add_sized([72.0, 20.0], egui::Label::new(gain.color(gain_color)));
                    let ch = e.choice.map_or_else(|| "—".to_string(), choice_letter);
                    ui.add_sized([28.0, 20.0], egui::Label::new(theme::dim(ch)));
                });
            });
        }
    });
}

pub fn choice_letter(c: usize) -> String {
    ["A", "B", "C", "D"]
        .get(c)
        .map_or_else(|| "?".to_string(), |s| (*s).to_string())
}

pub fn gain_label(gained: u32) -> String {
    format!("+{gained}")
}

pub fn show_end(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    widgets::heading(
        ui,
        t(lang, Key::FinalRanking),
        Some(t(lang, Key::ThanksForPlaying)),
    );
    widgets::card(ui, |ui| {
        for e in &s.ranking {
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                widgets::rank_pill(ui, e.rank);
                ui.label(egui::RichText::new(&e.name).size(theme::SIZE_BODY + 1.0));
                if e.player_id == s.my_id {
                    widgets::badge(ui, t(lang, Key::You), theme::ACCENT_HOVER);
                }
                if is_tied(&s.ranking, e) {
                    widgets::badge(ui, t(lang, Key::Tied), theme::WARNING);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::semi(
                        tf(lang, Key::Points, &[&e.score.to_string()]),
                        theme::SIZE_SUBTITLE,
                    ));
                });
            });
        }
    });
    ui.add_space(theme::GAP);
    let host = s.is_host();
    let mut again = false;
    let mut home = false;
    if host {
        let w = ui.available_width();
        ui.horizontal(|ui| {
            again = widgets::button(
                ui,
                t(lang, Key::NewGame),
                ButtonKind::Primary,
                Some(w * 0.5 - 6.0),
            )
            .clicked();
            home = widgets::button(
                ui,
                t(lang, Key::BackHome),
                ButtonKind::Secondary,
                Some(w * 0.5 - 6.0),
            )
            .clicked();
        });
    } else {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
            ui.label(theme::dim(t(lang, Key::WaitingHost)));
        });
        home = widgets::button(ui, t(lang, Key::Leave), ButtonKind::Secondary, None).clicked();
    }
    if again {
        s.back_to_lobby();
    }
    if home {
        app.go_home(None);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn points_gagnes() {
        assert_eq!(super::gain_label(850), "+850");
        assert_eq!(super::gain_label(0), "+0");
    }

    #[test]
    fn lettres_de_choix() {
        assert_eq!(super::choice_letter(0), "A");
        assert_eq!(super::choice_letter(3), "D");
        assert_eq!(super::choice_letter(9), "?");
    }
}

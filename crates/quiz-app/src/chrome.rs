use eframe::egui;
use quiz_core::{Lang, QuizState};

use crate::app::{QuizApp, Route, Screen};
use crate::i18n::{t, Key};
use crate::theme;
use crate::widgets::{self, ButtonKind, GhostIcon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitKind {
    InProgress,
    Host,
    Client,
}

impl QuitKind {
    fn message(self) -> Key {
        match self {
            QuitKind::InProgress => Key::QuitInProgress,
            QuitKind::Host => Key::QuitHost,
            QuitKind::Client => Key::QuitClient,
        }
    }
}

pub fn quit_confirmation(state: QuizState, session: Option<bool>) -> Option<QuitKind> {
    match session {
        // A session takes priority over the solo quiz state.
        Some(true) => Some(QuitKind::Host),
        Some(false) => Some(QuitKind::Client),
        None => matches!(
            state,
            QuizState::Generating | QuizState::Question { .. } | QuizState::Reveal { .. }
        )
        .then_some(QuitKind::InProgress),
    }
}

pub fn header(ctx: &egui::Context, app: &mut QuizApp, screen: Screen) {
    let lang = app.lang();
    let frame = egui::Frame::NONE
        .fill(theme::BG)
        .inner_margin(egui::Margin {
            left: 20,
            right: 20,
            top: 10,
            bottom: 6,
        });
    egui::TopBottomPanel::top("header")
        .frame(frame)
        .show_separator_line(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let quit = widgets::ghost_button(ui, t(lang, Key::Quit), GhostIcon::Close)
                    .on_hover_text(t(lang, Key::QuitTooltip));
                if quit.clicked() {
                    app.request_quit(ctx);
                }
                if screen == Screen::Home
                    && widgets::ghost_button(ui, t(lang, Key::Settings), GhostIcon::None).clicked()
                {
                    app.enter(Route::Settings, ctx);
                }
            });
        });
}

pub fn confirm_modal(ctx: &egui::Context, kind: QuitKind, lang: Lang) -> Option<bool> {
    let mut result = None;
    let frame = egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(theme::card_radius())
        .inner_margin(egui::Margin::same(24));
    let modal = egui::Modal::new(egui::Id::new("quit_confirm"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(theme::semi(t(lang, Key::QuitTitle), theme::SIZE_SUBTITLE));
            ui.label(egui::RichText::new(t(lang, kind.message())).color(theme::TEXT_DIM));
            ui.add_space(theme::GAP);
            ui.horizontal(|ui| {
                let half = ui.available_width() / 2.0 - 6.0;
                if widgets::button(ui, t(lang, Key::Quit), ButtonKind::Primary, Some(half))
                    .clicked()
                {
                    result = Some(true);
                }
                if widgets::button(ui, t(lang, Key::Cancel), ButtonKind::Secondary, Some(half))
                    .clicked()
                {
                    result = Some(false);
                }
            });
        });
    if modal.should_close() {
        result = result.or(Some(false));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_selon_l_etat() {
        use QuizState::*;
        assert_eq!(quit_confirmation(Setup, None), None);
        assert_eq!(quit_confirmation(Finished, None), None);
        assert_eq!(
            quit_confirmation(Generating, None),
            Some(QuitKind::InProgress)
        );
        assert_eq!(
            quit_confirmation(Question { index: 0 }, None),
            Some(QuitKind::InProgress)
        );
        assert_eq!(
            quit_confirmation(Reveal { index: 1 }, None),
            Some(QuitKind::InProgress)
        );
    }

    #[test]
    fn confirmation_session() {
        assert_eq!(
            quit_confirmation(QuizState::Setup, Some(true)),
            Some(QuitKind::Host)
        );
        assert_eq!(
            quit_confirmation(QuizState::Setup, Some(false)),
            Some(QuitKind::Client)
        );
    }
}

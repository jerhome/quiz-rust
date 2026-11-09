use eframe::egui;
use quiz_core::Lang;

use crate::app::QuizApp;
use crate::i18n::{t, Key};
use crate::theme;
use crate::widgets::{self, ButtonKind};

pub fn show_spinner(ui: &mut egui::Ui, label: Key, lang: Lang) {
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.add(egui::Spinner::new().size(44.0).color(theme::ACCENT));
        ui.add_space(theme::SECTION_GAP);
        ui.label(theme::semi(t(lang, label), theme::SIZE_SUBTITLE));
        ui.label(theme::dim(t(lang, Key::GeneratingHint)));
    });
}

pub fn show_error(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    widgets::heading(ui, t(lang, Key::GenFailed), None);
    if let Some(msg) = &app.gen_error {
        widgets::error_card(ui, &msg.text(lang));
    }
    ui.add_space(theme::GAP);
    let w = ui.available_width();
    ui.horizontal(|ui| {
        let half = w * 0.5 - 6.0;
        if widgets::button(ui, t(lang, Key::Retry), ButtonKind::Primary, Some(half)).clicked() {
            app.retry(ui.ctx());
        } else if widgets::button(ui, t(lang, Key::Back), ButtonKind::Secondary, Some(half))
            .clicked()
        {
            app.back_to_setup();
        }
    });
}

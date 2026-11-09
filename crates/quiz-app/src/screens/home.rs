use eframe::egui::{self, text::LayoutJob, FontId, TextFormat};

use crate::app::{QuizApp, Route};
use crate::i18n::{t, Key};
use crate::theme;
use crate::widgets;

fn title_job() -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::new(theme::SIZE_DISPLAY, theme::semi_family());
    for (text, color) in [("Quiz ", theme::TEXT), ("Rust", theme::ACCENT_HOVER)] {
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    }
    job
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    ui.add_space(8.0);
    ui.label(title_job());
    ui.label(
        egui::RichText::new(t(lang, Key::Tagline))
            .size(theme::SIZE_SUBTITLE - 2.0)
            .color(theme::TEXT_DIM),
    );
    ui.add_space(theme::SECTION_GAP);
    if let Some(n) = &app.notice {
        widgets::error_card(ui, &n.text(lang));
        ui.add_space(theme::GAP);
    }
    let choices = [
        (Key::SoloTitle, Key::SoloDesc, Route::Solo),
        (Key::HostTitle, Key::HostDesc, Route::Host),
        (Key::JoinTitle, Key::JoinDesc, Route::Join),
    ];
    for (title, desc, route) in choices {
        if widgets::menu_card(ui, t(lang, title), t(lang, desc), true).clicked() {
            app.enter(route, ui.ctx());
        }
    }
}

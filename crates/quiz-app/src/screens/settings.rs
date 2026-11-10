use eframe::egui;
use quiz_llm::{validate_model_id, MODELS};

use crate::app::QuizApp;
use crate::backend::Backend;
use crate::i18n::{t, tf, Key};
use crate::screens::setup::{api_key_field, lang_options};
use crate::theme;
use crate::widgets::{self, ButtonKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelChoice {
    Known(&'static str),
    Other,
}

fn model_options(lang: quiz_core::Lang) -> Vec<(ModelChoice, String)> {
    let mut v: Vec<_> = MODELS
        .iter()
        .map(|(id, name)| (ModelChoice::Known(id), (*name).to_string()))
        .collect();
    v.push((ModelChoice::Other, t(lang, Key::ModelOther).to_string()));
    v
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    widgets::heading(ui, t(lang, Key::Settings), Some(t(lang, Key::SettingsSub)));
    widgets::card(ui, |ui| {
        widgets::field_label(ui, t(lang, Key::UiLanguage));
        widgets::segmented(ui, &mut app.settings.ui_lang, &lang_options());
        widgets::field_label(ui, t(lang, Key::QuestionLang));
        widgets::segmented(ui, &mut app.settings.quiz_lang, &lang_options());
        model_section(ui, app);
        api_key_field(ui, &mut app.form, lang);
        key_memory(ui, app);
        history_section(ui, app);
        if cfg!(target_os = "linux") {
            display_section(ui, app);
        }
    });
    ui.add_space(theme::GAP);
    if widgets::button(ui, t(lang, Key::Back), ButtonKind::Secondary, None).clicked() {
        app.go_home(None);
    }
}

fn model_section(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    widgets::field_label(ui, t(lang, Key::Model));
    let mut choice = if app.model_other {
        ModelChoice::Other
    } else {
        MODELS
            .iter()
            .find(|(id, _)| *id == app.settings.model)
            .map_or(ModelChoice::Other, |(id, _)| ModelChoice::Known(id))
    };
    if widgets::segmented(ui, &mut choice, &model_options(lang)) {
        match choice {
            ModelChoice::Known(id) => {
                app.model_other = false;
                app.settings.model = id.to_string();
            }
            ModelChoice::Other => {
                app.model_other = true;
                app.model_text = app.settings.model.clone();
            }
        }
    }
    if app.model_other {
        widgets::text_field(ui, &mut app.model_text, t(lang, Key::ModelIdHint), false);
        match validate_model_id(&app.model_text) {
            Ok(id) => app.settings.model = id,
            Err(e) => widgets::error_text(ui, &e.message(lang)),
        }
    }
}

fn key_memory(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    ui.add_space(4.0);
    if app.key_from_env() {
        // Key comes from the environment: checkbox disabled, the key is never written to the file.
        ui.add_enabled_ui(false, |ui| {
            let mut off = false;
            widgets::checkbox(ui, &mut off, t(lang, Key::RememberSaved));
        });
        ui.label(theme::dim(t(lang, Key::EnvVarNotice)));
    } else {
        widgets::checkbox(
            ui,
            &mut app.settings.remember_key,
            t(lang, Key::RememberSaved),
        );
        if app.settings.remember_key {
            ui.label(theme::dim(t(lang, Key::RememberWarn)));
        }
    }
    if app.save_failed {
        widgets::error_text(ui, t(lang, Key::SaveFailed));
    }
    if widgets::button(ui, t(lang, Key::ForgetSaved), ButtonKind::Secondary, None).clicked() {
        app.forget_key();
    }
}

fn history_section(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    ui.add_space(theme::GAP);
    ui.separator();
    ui.label(theme::dim(t(lang, Key::HistoryHelp)));
    let n = app.history.len().to_string();
    if !app.confirm_clear_history {
        let label = tf(lang, Key::ClearHistory, &[&n]);
        let b = widgets::button_enabled(
            ui,
            &label,
            ButtonKind::Secondary,
            None,
            !app.history.is_empty(),
        );
        app.confirm_clear_history = b.clicked();
        return;
    }
    ui.label(tf(lang, Key::ClearHistoryAsk, &[&n]));
    ui.horizontal(|ui| {
        let half = ui.available_width() / 2.0 - 6.0;
        if widgets::button(
            ui,
            t(lang, Key::ClearHistoryYes),
            ButtonKind::Primary,
            Some(half),
        )
        .clicked()
        {
            app.clear_history();
        }
        if widgets::button(ui, t(lang, Key::Cancel), ButtonKind::Secondary, Some(half)).clicked() {
            app.confirm_clear_history = false;
        }
    });
}

fn display_section(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    ui.add_space(theme::GAP);
    ui.separator();
    ui.label(theme::semi(t(lang, Key::Display), theme::SIZE_SUBTITLE));
    let options: Vec<_> = Backend::ALL
        .iter()
        .map(|b| (*b, b.label().to_string()))
        .collect();
    widgets::segmented(ui, &mut app.settings.display_backend, &options);
    ui.label(theme::dim(t(lang, Key::DisplayHelp)));
    let current = tf(
        lang,
        Key::DisplayCurrent,
        &[app.active_backend.backend.label()],
    );
    widgets::badge(ui, &current, theme::TEXT_DIM);
    if app.active_backend.fell_back {
        ui.label(theme::dim(t(lang, Key::WaylandFallback)));
    }
    if app.restart_needed() && app.restart_dismissed != Some(app.settings.display_backend) {
        restart_banner(ui, app);
    }
    if app.restart_failed {
        widgets::error_text(ui, t(lang, Key::RestartFailed));
    }
}

fn restart_banner(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    widgets::card_styled(ui, theme::tint(theme::ACCENT, 0.1), theme::ACCENT, |ui| {
        ui.label(t(lang, Key::RestartNeeded));
        ui.horizontal(|ui| {
            let half = ui.available_width() / 2.0 - 6.0;
            if widgets::button(
                ui,
                t(lang, Key::RestartNow),
                ButtonKind::Primary,
                Some(half),
            )
            .clicked()
            {
                app.request_restart(ui.ctx());
            }
            let later = widgets::button(ui, t(lang, Key::Later), ButtonKind::Secondary, Some(half));
            if later.clicked() {
                app.restart_dismissed = Some(app.settings.display_backend);
            }
        });
    });
}

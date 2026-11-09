use eframe::egui;
use quiz_core::{Difficulty, Lang, QuizConfig, COUNT_RANGE};

use crate::app::{QuizApp, Route};
use crate::i18n::{t, tf, Key, Msg};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::{self, ButtonKind};

pub const TIME_CHOICES: [Option<u32>; 7] = [
    Some(10),
    Some(15),
    Some(20),
    Some(30),
    Some(45),
    Some(60),
    None,
];
pub const DEFAULT_TIME: Option<u32> = Some(20);

pub fn short_time_label(lang: Lang, t_: Option<u32>) -> String {
    t_.map_or_else(
        || t(lang, Key::Unlimited).to_string(),
        |s| tf(lang, Key::Seconds, &[&s.to_string()]),
    )
}

pub fn time_from_settings(secs: u32) -> Option<u32> {
    let choice = if secs == 0 { None } else { Some(secs) };
    if TIME_CHOICES.contains(&choice) {
        choice
    } else {
        DEFAULT_TIME
    }
}

pub struct SetupForm {
    pub theme: String,
    pub difficulty: Difficulty,
    pub count: u8,
    pub api_key: String,
    pub name: String,
    pub time_limit: Option<u32>,
    pub show_key: bool,
    pub error: Option<Msg>,
}

impl SetupForm {
    pub fn from_settings(s: &Settings, api_key: String) -> Self {
        Self {
            theme: String::new(),
            difficulty: s.difficulty,
            count: s.count,
            api_key,
            name: s.name.clone(),
            time_limit: time_from_settings(s.time_limit_secs),
            show_key: false,
            error: None,
        }
    }

    pub fn time_limit_duration(&self) -> Option<std::time::Duration> {
        self.time_limit
            .map(|s| std::time::Duration::from_secs(u64::from(s)))
    }

    pub fn config(&self, language: Lang) -> QuizConfig {
        QuizConfig {
            theme: self.theme.trim().to_string(),
            difficulty: self.difficulty,
            count: self.count,
            language,
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let host = app.route == Route::Host;
    let (title, sub) = if host {
        (Key::HostSetupTitle, Key::HostSetupSub)
    } else {
        (Key::NewQuizTitle, Key::NewQuizSub)
    };
    widgets::heading(ui, t(lang, title), Some(t(lang, sub)));
    let mut submit = false;
    widgets::card(ui, |ui| {
        widgets::field_label(ui, t(lang, Key::Topic));
        let hint = t(lang, Key::TopicHint);
        let theme = widgets::text_field(ui, &mut app.form.theme, hint, false);
        submit = theme.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        widgets::field_label(ui, t(lang, Key::Difficulty));
        let opts: Vec<_> = Difficulty::ALL
            .iter()
            .map(|d| (*d, d.label(lang).to_string()))
            .collect();
        widgets::segmented(ui, &mut app.form.difficulty, &opts);
        let count = app.form.count.to_string();
        widgets::field_label(ui, &tf(lang, Key::QuestionCount, &[&count]));
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(&mut app.form.count, COUNT_RANGE).show_value(false));
        widgets::field_label(ui, t(lang, Key::QuestionLang));
        widgets::segmented(ui, &mut app.settings.quiz_lang, &lang_options());
        if host {
            host_fields(ui, &mut app.form, lang);
        }
        api_key_field(ui, &mut app.form, lang);
        if let Some(err) = &app.form.error {
            widgets::error_text(ui, &err.text(lang));
        }
    });
    ui.add_space(theme::GAP);
    let w = ui.available_width();
    ui.horizontal(|ui| {
        let gen = widgets::button(
            ui,
            t(lang, Key::Generate),
            ButtonKind::Primary,
            Some(w * 0.66),
        );
        if gen.clicked() {
            submit = true;
        }
        let back_w = w * 0.34 - theme::GAP;
        if widgets::button(ui, t(lang, Key::Back), ButtonKind::Secondary, Some(back_w)).clicked() {
            app.leave_setup();
        }
    });
    if submit {
        app.submit_setup(ui.ctx());
    }
}

pub fn lang_options() -> Vec<(Lang, String)> {
    Lang::ALL
        .iter()
        .map(|l| (*l, l.native_name().to_string()))
        .collect()
}

fn host_fields(ui: &mut egui::Ui, form: &mut SetupForm, lang: Lang) {
    widgets::field_label(ui, t(lang, Key::YourName));
    widgets::text_field(ui, &mut form.name, t(lang, Key::NameHint), false);
    widgets::field_label(ui, t(lang, Key::TimePerQuestion));
    let opts: Vec<_> = TIME_CHOICES
        .iter()
        .map(|c| (*c, short_time_label(lang, *c)))
        .collect();
    widgets::segmented(ui, &mut form.time_limit, &opts);
    if form.time_limit.is_none() {
        ui.label(theme::dim(t(lang, Key::UnlimitedHint)));
    }
}

pub fn api_key_field(ui: &mut egui::Ui, form: &mut SetupForm, lang: Lang) {
    widgets::field_label(ui, t(lang, Key::ApiKeyLabel));
    ui.horizontal(|ui| {
        let w = ui.available_width() - 120.0 - theme::GAP;
        widgets::text_field_w(ui, &mut form.api_key, "sk-ant-…", !form.show_key, w);
        let label = if form.show_key { Key::Hide } else { Key::Show };
        if widgets::button(ui, t(lang, label), ButtonKind::Secondary, Some(120.0)).clicked() {
            form.show_key = !form.show_key;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libelles_temps() {
        assert_eq!(short_time_label(Lang::Fr, Some(20)), "20 s");
        assert_eq!(short_time_label(Lang::Fr, None), "Illimité");
        assert_eq!(short_time_label(Lang::En, None), "Unlimited");
        assert!(TIME_CHOICES.contains(&DEFAULT_TIME));
    }

    #[test]
    fn temps_enregistre() {
        assert_eq!(time_from_settings(0), None);
        assert_eq!(time_from_settings(45), Some(45));
        assert_eq!(time_from_settings(17), DEFAULT_TIME);
    }

    #[test]
    fn config_depuis_formulaire() {
        let mut f = SetupForm::from_settings(&Settings::default(), "k".into());
        f.theme = "  Rome ".into();
        f.count = 8;
        let c = f.config(Lang::En);
        assert_eq!(c.theme, "Rome");
        assert_eq!(c.count, 8);
        assert_eq!(c.language, Lang::En);
    }
}

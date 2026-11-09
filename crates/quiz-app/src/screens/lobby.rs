use eframe::egui;

use crate::app::QuizApp;
use crate::i18n::{t, tf, Key};
use crate::theme;
use crate::widgets::{self, ButtonKind};

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    widgets::heading(ui, t(lang, Key::Lobby), Some(t(lang, Key::LobbySub)));
    if s.is_host() {
        let ip = s
            .local_ip
            .map_or_else(|| "?".to_string(), |i| i.to_string());
        widgets::card_styled(ui, theme::tint(theme::ACCENT, 0.1), theme::ACCENT, |ui| {
            ui.label(theme::dim(t(lang, Key::AddressToShare)));
            ui.label(theme::semi(
                format!("{ip}:{}", s.port),
                theme::SIZE_SUBTITLE + 4.0,
            ));
        });
        ui.add_space(theme::GAP);
    }
    widgets::card(ui, |ui| {
        ui.label(theme::semi(
            tf(lang, Key::PlayersCount, &[&s.players.len().to_string()]),
            theme::SIZE_SUBTITLE,
        ));
        for p in &s.players {
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                let color = if p.is_host {
                    theme::ACCENT_HOVER
                } else {
                    theme::SUCCESS
                };
                widgets::avatar(ui, &p.name, color);
                ui.label(egui::RichText::new(&p.name).size(theme::SIZE_BODY));
                if p.is_host {
                    widgets::badge(ui, t(lang, Key::HostBadge), theme::ACCENT_HOVER);
                }
            });
        }
    });
    ui.add_space(theme::GAP);
    let mut leave = false;
    let mut configure = false;
    if s.is_host() {
        let w = ui.available_width();
        ui.horizontal(|ui| {
            if s.can_launch() || s.launched {
                let b = widgets::button_enabled(
                    ui,
                    t(lang, Key::Launch),
                    ButtonKind::Primary,
                    Some(w * 0.66),
                    s.can_launch(),
                );
                if b.clicked() {
                    s.start_game();
                }
            } else {
                let b = widgets::button(
                    ui,
                    t(lang, Key::ConfigureNext),
                    ButtonKind::Primary,
                    Some(w * 0.66),
                );
                configure = b.clicked();
            }
            let b = widgets::button(
                ui,
                t(lang, Key::Cancel),
                ButtonKind::Secondary,
                Some(w * 0.34 - theme::GAP),
            );
            leave = b.clicked();
        });
    } else {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
            let msg = if s.joined_mid_game() {
                t(lang, Key::MidGame)
            } else {
                t(lang, Key::WaitingHost)
            };
            ui.label(theme::dim(msg));
        });
        leave = widgets::button(ui, t(lang, Key::Leave), ButtonKind::Secondary, None).clicked();
    }
    if configure {
        app.configure_next_game();
    }
    if leave {
        app.go_home(None);
    }
}

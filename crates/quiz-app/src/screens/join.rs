use std::time::Instant;

use eframe::egui;
use quiz_net::{DiscoveredHost, DiscoveryHandle};

use crate::app::QuizApp;
use crate::i18n::{t, Key, Msg};
use crate::multi::{parse_addr, show_no_games_hint, upsert_host, visible_hosts, HOST_TTL};
use crate::theme;
use crate::widgets::{self, ButtonKind};

#[derive(Default)]
pub struct JoinState {
    pub manual_addr: String,
    pub error: Option<Msg>,
    pub connecting: bool,
    pub discovery: Option<DiscoveryHandle>,
    hosts: Vec<DiscoveredHost>,
    pub started: Option<Instant>,
    focused_for_hint: bool,
}

impl JoinState {
    pub fn drain_hosts(&mut self) {
        let Some(d) = &mut self.discovery else { return };
        while let Ok(h) = d.hosts.try_recv() {
            upsert_host(&mut self.hosts, h);
        }
    }

    fn visible(&self) -> Vec<DiscoveredHost> {
        visible_hosts(&self.hosts, Instant::now(), HOST_TTL)
            .into_iter()
            .cloned()
            .collect()
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    widgets::heading(ui, t(lang, Key::JoinHeading), Some(t(lang, Key::JoinSub)));
    let mut chosen = None;
    widgets::card(ui, |ui| {
        widgets::field_label(ui, t(lang, Key::YourName));
        widgets::text_field(ui, &mut app.form.name, t(lang, Key::NameHintJoin), false);
    });
    ui.add_space(theme::GAP);
    ui.label(theme::semi(
        t(lang, Key::DetectedGames),
        theme::SIZE_SUBTITLE,
    ));
    let hosts = app.join.visible();
    let show_hint = app
        .join
        .started
        .is_some_and(|s| show_no_games_hint(s, Instant::now(), !hosts.is_empty()));
    if hosts.is_empty() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
            ui.label(theme::dim(t(lang, Key::Searching)));
        });
    }
    if show_hint {
        // A soft hint, not an error: discovery is often blocked on corporate networks.
        widgets::card_styled(ui, theme::tint(theme::ACCENT, 0.08), theme::BORDER, |ui| {
            ui.label(t(lang, Key::NoGamesFound));
        });
    }
    for h in &hosts {
        if widgets::menu_card(ui, &h.name, &h.addr.to_string(), !app.join.connecting).clicked() {
            chosen = Some(h.addr);
        }
    }
    ui.add_space(theme::GAP);
    widgets::card(ui, |ui| {
        widgets::field_label(ui, t(lang, Key::ManualConnect));
        ui.horizontal(|ui| {
            let w = ui.available_width() - 160.0 - theme::GAP;
            let field =
                widgets::text_field_w(ui, &mut app.join.manual_addr, "192.168.1.5:7878", false, w);
            if show_hint && !app.join.focused_for_hint {
                field.request_focus();
                app.join.focused_for_hint = true;
            }
            let b = widgets::button_enabled(
                ui,
                t(lang, Key::Connect),
                ButtonKind::Primary,
                Some(160.0),
                !app.join.connecting,
            );
            if b.clicked() {
                match parse_addr(&app.join.manual_addr) {
                    Ok(a) => chosen = Some(a),
                    Err(e) => app.join.error = Some(e),
                }
            }
        });
        if let Some(e) = &app.join.error {
            widgets::error_text(ui, &e.text(lang));
        }
        if app.join.connecting {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
                ui.label(theme::dim(t(lang, Key::Connecting)));
            });
        }
    });
    if let Some(addr) = chosen {
        app.connect(ui.ctx(), addr);
    }
    ui.add_space(theme::GAP);
    if widgets::button(ui, t(lang, Key::Back), ButtonKind::Secondary, None).clicked() {
        app.go_home(None);
    }
}

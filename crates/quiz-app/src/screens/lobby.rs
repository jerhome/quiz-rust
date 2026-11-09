use eframe::egui;

use crate::app::QuizApp;
use crate::i18n::{t, tf, Key};
use crate::multi;
use crate::theme;
use crate::widgets::{self, ButtonKind, GhostIcon};
use quiz_core::Lang;
use quiz_net::LocalAddr;

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    widgets::heading(ui, t(lang, Key::Lobby), Some(t(lang, Key::LobbySub)));
    if s.is_host() {
        address_card(ui, &s.addrs, s.port, lang);
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

fn address_card(ui: &mut egui::Ui, addrs: &[LocalAddr], port: u16, lang: Lang) {
    let primary = multi::primary_addr(addrs);
    widgets::card_styled(ui, theme::tint(theme::ACCENT, 0.1), theme::ACCENT, |ui| {
        ui.label(theme::dim(t(lang, Key::AddressToShare)));
        match primary {
            Some(p) => {
                let text = multi::format_addr(p.ip, port);
                ui.horizontal(|ui| {
                    ui.label(theme::semi(&text, theme::SIZE_SUBTITLE + 4.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        copy_button(ui, &text, lang);
                    });
                });
            }
            None => {
                ui.label(theme::dim(t(lang, Key::NoLanAddress)));
            }
        }
        let others: Vec<_> = addrs
            .iter()
            .filter(|a| primary.is_none_or(|p| p.ip != a.ip))
            .collect();
        if !others.is_empty() {
            let title = egui::RichText::new(t(lang, Key::OtherAddresses)).color(theme::TEXT_DIM);
            egui::CollapsingHeader::new(title)
                .default_open(primary.is_none())
                .show(ui, |ui| {
                    for a in others {
                        other_address_row(ui, a, port, lang);
                    }
                });
        }
    });
}

fn other_address_row(ui: &mut egui::Ui, a: &LocalAddr, port: u16, lang: Lang) {
    ui.horizontal(|ui| {
        let color = if a.is_likely_virtual {
            theme::TEXT_DIM
        } else {
            theme::TEXT
        };
        let text = format!(
            "{} \u{2014} {}",
            multi::format_addr(a.ip, port),
            a.interface_name
        );
        ui.label(egui::RichText::new(text).color(color));
        if a.is_likely_virtual {
            widgets::badge(ui, t(lang, Key::VirtualBadge), theme::WARNING);
        }
    });
}

fn copy_button(ui: &mut egui::Ui, text: &str, lang: Lang) {
    let id = egui::Id::new("copied_at");
    let now = ui.input(|i| i.time);
    let copied_at: Option<f64> = ui.data(|d| d.get_temp(id));
    let recent = copied_at.is_some_and(|t0| now - t0 < 2.0);
    let label = if recent { Key::Copied } else { Key::Copy };
    if widgets::ghost_button(ui, t(lang, label), GhostIcon::None).clicked() {
        ui.ctx().copy_text(text.to_string());
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(2100));
    }
}

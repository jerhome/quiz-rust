use eframe::egui;

use crate::app::QuizApp;
use crate::i18n::{t, tf, Key};
use crate::multi;
use crate::theme;
use crate::web;
use crate::widgets::{self, ButtonKind, GhostIcon};
use quiz_core::Lang;
use quiz_net::{Device, LocalAddr};

const TWO_COLUMNS_MIN_WIDTH: f32 = 760.0;
const ACTIONS_HEIGHT: f32 = 76.0;
const QR_SIZE: f32 = 150.0;

pub fn show(ui: &mut egui::Ui, app: &mut QuizApp) {
    let lang = app.lang();
    let Some(s) = app.session.as_mut() else {
        return;
    };
    widgets::heading(ui, t(lang, Key::Lobby), Some(t(lang, Key::LobbySub)));
    let mut outcome = Outcome::default();
    if s.is_host() {
        let body_h = body_height(ui, ACTIONS_HEIGHT);
        if ui.available_width() >= TWO_COLUMNS_MIN_WIDTH {
            ui.columns(2, |cols| {
                scroll(&mut cols[0], "lobby_info", body_h + ACTIONS_HEIGHT, |ui| {
                    host_info(ui, s, lang)
                });
                scroll(&mut cols[1], "lobby_players", body_h, |ui| {
                    players_card(ui, s, lang)
                });
                outcome = host_actions(&mut cols[1], s, lang);
            });
        } else {
            scroll(ui, "lobby_all", body_h, |ui| {
                host_info(ui, s, lang);
                players_card(ui, s, lang);
            });
            outcome = host_actions(ui, s, lang);
        }
    } else {
        players_card(ui, s, lang);
        ui.add_space(theme::GAP);
        outcome.leave = guest_footer(ui, s, lang);
    }
    if outcome.configure {
        app.configure_next_game();
    }
    if outcome.leave {
        app.go_home(None);
    }
}

#[derive(Default)]
struct Outcome {
    leave: bool,
    configure: bool,
}

/// Height left in the visible viewport below the cursor, minus a reserved bottom strip.
fn body_height(ui: &egui::Ui, reserved: f32) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top() - reserved).max(160.0)
}

fn scroll(ui: &mut egui::Ui, salt: &str, max_h: f32, add: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .max_height(max_h)
        .auto_shrink([false, true])
        .id_salt(salt)
        .show(ui, add);
}

fn host_info(ui: &mut egui::Ui, s: &multi::Session, lang: Lang) {
    address_card(ui, &s.addrs, s.port, lang);
    ui.add_space(theme::GAP);
    web_card(ui, &web_card_state(s.web_port, &s.addrs), lang);
    ui.add_space(theme::GAP);
}

fn players_card(ui: &mut egui::Ui, s: &multi::Session, lang: Lang) {
    widgets::card(ui, |ui| {
        ui.label(theme::semi(
            tf(
                lang,
                Key::PlayersCount,
                &[&web::connected_count(&s.players).to_string()],
            ),
            theme::SIZE_SUBTITLE,
        ));
        for p in &s.players {
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                if !p.connected {
                    ui.set_opacity(0.45);
                }
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
                if p.device == Device::Web {
                    widgets::phone_icon(ui, theme::TEXT_DIM).on_hover_text(t(lang, Key::PhoneTag));
                }
                if !p.connected {
                    ui.label(theme::dim(t(lang, Key::DisconnectedTag)));
                }
            });
        }
    });
}

fn host_actions(ui: &mut egui::Ui, s: &mut multi::Session, lang: Lang) -> Outcome {
    let mut out = Outcome::default();
    ui.add_space(theme::GAP);
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
            out.configure = b.clicked();
        }
        let b = widgets::button(
            ui,
            t(lang, Key::Cancel),
            ButtonKind::Secondary,
            Some(w * 0.34 - theme::GAP),
        );
        out.leave = b.clicked();
    });
    out
}

fn guest_footer(ui: &mut egui::Ui, s: &multi::Session, lang: Lang) -> bool {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(18.0).color(theme::ACCENT));
        let msg = if s.joined_mid_game() {
            t(lang, Key::MidGame)
        } else {
            t(lang, Key::WaitingHost)
        };
        ui.label(theme::dim(msg));
    });
    widgets::button(ui, t(lang, Key::Leave), ButtonKind::Secondary, None).clicked()
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
    let id = egui::Id::new(("copied_at", text));
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

enum WebCard {
    Url(String),
    NoAddress,
    PortUnavailable,
}

fn web_card_state(web_port: Option<u16>, addrs: &[LocalAddr]) -> WebCard {
    match web_port {
        None => WebCard::PortUnavailable,
        Some(port) => match web::web_url(addrs, port) {
            Some(url) => WebCard::Url(url),
            None => WebCard::NoAddress,
        },
    }
}

fn web_card(ui: &mut egui::Ui, card: &WebCard, lang: Lang) {
    widgets::card(ui, |ui| {
        ui.label(theme::semi(
            t(lang, Key::PhoneJoinTitle),
            theme::SIZE_SUBTITLE,
        ));
        match card {
            WebCard::Url(url) => {
                ui.label(theme::dim(t(lang, Key::PhoneJoinHint)));
                ui.horizontal(|ui| {
                    if let Some((side, dark)) = web::qr_modules(url) {
                        widgets::qr_code(ui, side, &dark, QR_SIZE);
                    }
                    ui.vertical(|ui| {
                        ui.label(theme::semi(url, theme::SIZE_BODY));
                        copy_button(ui, url, lang);
                    });
                });
            }
            WebCard::NoAddress => {
                ui.label(theme::dim(t(lang, Key::WebNeedsAddress)));
            }
            WebCard::PortUnavailable => {
                let msg = tf(
                    lang,
                    Key::WebPortUnavailable,
                    &[&quiz_net::DEFAULT_WEB_PORT.to_string()],
                );
                ui.label(egui::RichText::new(msg).color(theme::WARNING));
            }
        }
    });
}

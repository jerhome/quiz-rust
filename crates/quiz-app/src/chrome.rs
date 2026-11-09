use eframe::egui;
use quiz_core::{Lang, QuizState};

use crate::app::{QuizApp, Route, Screen};
use crate::i18n::{t, Key};
use crate::theme;
use crate::widgets::{self, ButtonKind, GhostIcon, WindowIcon};

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
    let custom = app.custom_titlebar;
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let frame = egui::Frame::NONE
        .fill(theme::BG)
        .inner_margin(egui::Margin {
            left: 20,
            right: if custom { 8 } else { 20 },
            top: if custom { 4 } else { 10 },
            bottom: if custom { 2 } else { 6 },
        });
    egui::TopBottomPanel::top("header")
        .frame(frame)
        .show_separator_line(false)
        .show(ctx, |ui| {
            if custom {
                title_bar_drag(ui, ctx, maximized);
            }
            ui.horizontal(|ui| {
                if custom {
                    title_bar_brand(ui, app);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let quit = widgets::ghost_button(ui, t(lang, Key::Quit), GhostIcon::Close)
                        .on_hover_text(t(lang, Key::QuitTooltip));
                    if quit.clicked() {
                        app.request_quit(ctx);
                    }
                    if custom {
                        window_controls(ui, ctx, lang, maximized);
                    }
                    if screen == Screen::Home
                        && widgets::ghost_button(ui, t(lang, Key::Settings), GhostIcon::None)
                            .clicked()
                    {
                        app.enter(Route::Settings, ctx);
                    }
                });
            });
        });
}

// The whole bar is a drag handle. It is registered before the buttons so they stay on top
// and keep receiving their own clicks.
fn title_bar_drag(ui: &mut egui::Ui, ctx: &egui::Context, maximized: bool) {
    // Same height as the row below (interact_size.y).
    let rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
    );
    let bar = ui.interact(
        rect,
        ui.id().with("title_drag"),
        egui::Sense::click_and_drag(),
    );
    if bar.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if bar.double_clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

fn title_bar_brand(ui: &mut egui::Ui, app: &QuizApp) {
    if let Some(tex) = &app.title_icon {
        ui.add(egui::Image::new((tex.id(), egui::vec2(18.0, 18.0))));
    }
    ui.label(
        egui::RichText::new("Quiz Rust")
            .size(theme::SIZE_CAPTION)
            .color(theme::TEXT_DIM),
    );
}

// Added right-to-left: maximize/restore first (next to Quit), then minimize.
fn window_controls(ui: &mut egui::Ui, ctx: &egui::Context, lang: Lang, maximized: bool) {
    let (icon, hint) = if maximized {
        (WindowIcon::Restore, Key::Restore)
    } else {
        (WindowIcon::Maximize, Key::Maximize)
    };
    if widgets::window_button(ui, icon, t(lang, hint)).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if widgets::window_button(ui, WindowIcon::Minimize, t(lang, Key::Minimize)).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

const EDGE: f32 = 6.0;
const CORNER: f32 = 12.0;

/// Resize direction for a pointer position inside a window of `size`, if on a handle.
pub fn resize_direction(pos: egui::Pos2, size: egui::Vec2) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection::*;
    let near = |v: f32, max: f32, d: f32| (v < d, v > max - d);
    let (cl, cr) = near(pos.x, size.x, CORNER);
    let (ct, cb) = near(pos.y, size.y, CORNER);
    match ((cl, cr), (ct, cb)) {
        ((true, _), (true, _)) => return Some(NorthWest),
        ((_, true), (true, _)) => return Some(NorthEast),
        ((true, _), (_, true)) => return Some(SouthWest),
        ((_, true), (_, true)) => return Some(SouthEast),
        _ => {}
    }
    let (el, er) = near(pos.x, size.x, EDGE);
    let (et, eb) = near(pos.y, size.y, EDGE);
    match (el, er, et, eb) {
        (true, ..) => Some(West),
        (_, true, ..) => Some(East),
        (_, _, true, _) => Some(North),
        (.., true) => Some(South),
        _ => None,
    }
}

fn resize_cursor(dir: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::{CursorIcon as C, ResizeDirection::*};
    match dir {
        North => C::ResizeNorth,
        South => C::ResizeSouth,
        East => C::ResizeEast,
        West => C::ResizeWest,
        NorthEast => C::ResizeNorthEast,
        NorthWest => C::ResizeNorthWest,
        SouthEast => C::ResizeSouthEast,
        SouthWest => C::ResizeSouthWest,
    }
}

/// Resize handles and 1 px outline for the undecorated window (called after all panels,
/// so the cursor set here wins over the widgets underneath).
pub fn window_frame(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ctx.content_rect();
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("window_border"));
    ctx.layer_painter(layer).rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0_f32, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let Some(pos) = ctx.pointer_hover_pos() else {
        return;
    };
    if let Some(dir) = resize_direction((pos - rect.min).to_pos2(), rect.size()) {
        ctx.set_cursor_icon(resize_cursor(dir));
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

pub fn footer(ctx: &egui::Context, app: &mut QuizApp) {
    let lang = app.lang();
    let frame = egui::Frame::NONE
        .fill(theme::BG)
        .inner_margin(egui::Margin {
            left: 20,
            right: 20,
            top: 6,
            bottom: 8,
        });
    egui::TopBottomPanel::bottom("footer")
        .frame(frame)
        .show_separator_line(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if version_link(ui, lang) {
                    app.link_failed =
                        crate::links::open_url(&crate::links::current_release_url()).is_err();
                }
                if app.link_failed {
                    ui.label(
                        egui::RichText::new(t(lang, Key::OpenFailed))
                            .size(theme::SIZE_CAPTION)
                            .color(theme::WARNING),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let credit = egui::RichText::new(t(lang, Key::MadeBy))
                        .size(theme::SIZE_CAPTION)
                        .color(theme::TEXT_DIM);
                    // Truncates instead of overlapping the version on a narrow window.
                    ui.add(egui::Label::new(credit).truncate());
                });
            });
        });
}

/// Clickable version label; returns true when clicked.
fn version_link(ui: &mut egui::Ui, lang: Lang) -> bool {
    let text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let font = egui::FontId::proportional(theme::SIZE_CAPTION);
    let galley = ui.painter().layout_no_wrap(text, font, theme::TEXT_DIM);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::click());
    let hovered = resp.hovered();
    let color = if hovered {
        theme::ACCENT_HOVER
    } else {
        theme::TEXT_DIM
    };
    ui.painter().galley(rect.min, galley, color);
    if hovered {
        let y = rect.bottom();
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0_f32, color),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(t(lang, Key::ViewRelease))
        .clicked()
}

pub fn confirm_modal(
    ctx: &egui::Context,
    kind: QuitKind,
    lang: Lang,
    restart: bool,
) -> Option<bool> {
    let (title, confirm) = if restart {
        (Key::RestartTitle, Key::RestartConfirm)
    } else {
        (Key::QuitTitle, Key::Quit)
    };
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
            ui.label(theme::semi(t(lang, title), theme::SIZE_SUBTITLE));
            ui.label(egui::RichText::new(t(lang, kind.message())).color(theme::TEXT_DIM));
            ui.add_space(theme::GAP);
            ui.horizontal(|ui| {
                let half = ui.available_width() / 2.0 - 6.0;
                if widgets::button(ui, t(lang, confirm), ButtonKind::Primary, Some(half)).clicked()
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
    fn zones_de_redimensionnement() {
        use egui::ResizeDirection::*;
        let size = egui::vec2(960.0, 720.0);
        let dir = |x: f32, y: f32| resize_direction(egui::pos2(x, y), size);
        assert_eq!(dir(480.0, 360.0), None);
        assert_eq!(dir(2.0, 300.0), Some(West));
        assert_eq!(dir(958.0, 300.0), Some(East));
        assert_eq!(dir(400.0, 2.0), Some(North));
        assert_eq!(dir(400.0, 718.0), Some(South));
        assert_eq!(dir(3.0, 3.0), Some(NorthWest));
        assert_eq!(dir(10.0, 9.0), Some(NorthWest));
        assert_eq!(dir(950.0, 5.0), Some(NorthEast));
        assert_eq!(dir(5.0, 715.0), Some(SouthWest));
        assert_eq!(dir(955.0, 710.0), Some(SouthEast));
        // beyond the edge band and not in a corner: content area
        assert_eq!(dir(10.0, 300.0), None);
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

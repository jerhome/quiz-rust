use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, FontId, Frame, Margin, Rect, Response, RichText,
    Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder,
};

use crate::i18n::{tf, Key};
use crate::theme as t;

pub fn page(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail = ui.available_rect_before_wrap();
            let w = (avail.width() - 48.0).clamp(240.0, max_width);
            let left = avail.center().x - w / 2.0;
            let col = Rect::from_min_max(pos2(left, avail.top()), pos2(left + w, avail.bottom()));
            ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
                ui.add_space(t::SECTION_GAP + 8.0);
                add(ui);
                ui.add_space(t::SECTION_GAP + 8.0);
            });
        });
}

pub fn heading(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.label(t::semi(title, t::SIZE_TITLE));
    if let Some(s) = subtitle {
        ui.label(RichText::new(s).size(t::SIZE_BODY + 1.0).color(t::TEXT_DIM));
    }
    ui.add_space(t::GAP);
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_styled(ui, t::SURFACE, t::BORDER, add)
}

pub fn card_styled<R>(
    ui: &mut Ui,
    fill: Color32,
    border: Color32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let inner_width = ui.available_width() - 48.0 - 2.0;
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(t::card_radius())
        .inner_margin(Margin::same(24))
        .shadow(Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(70),
        })
        .show(ui, |ui| {
            ui.set_width(inner_width);
            add(ui)
        })
        .inner
}

pub fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text)
            .size(t::SIZE_CAPTION + 1.0)
            .color(t::TEXT_DIM),
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
}

pub fn button(ui: &mut Ui, text: &str, kind: ButtonKind, width: Option<f32>) -> Response {
    button_enabled(ui, text, kind, width, true)
}

pub fn button_enabled(
    ui: &mut Ui,
    text: &str,
    kind: ButtonKind,
    width: Option<f32>,
    enabled: bool,
) -> Response {
    let w = width.unwrap_or_else(|| ui.available_width());
    button_impl(ui, text, kind, w, 46.0, enabled)
}

fn button_impl(
    ui: &mut Ui,
    text: &str,
    kind: ButtonKind,
    w: f32,
    height: f32,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), sense);
    let hover =
        ui.ctx()
            .animate_value_with_time(resp.id, f32::from(resp.hovered() && enabled), 0.12);
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let (fill, stroke, text_color) = match kind {
        ButtonKind::Primary => {
            let base = t::ACCENT.lerp_to_gamma(t::ACCENT_HOVER, hover);
            let base = if pressed { t::ACCENT_PRESSED } else { base };
            (base, Stroke::NONE, Color32::WHITE)
        }
        ButtonKind::Secondary => (
            t::SURFACE.lerp_to_gamma(t::SURFACE_2, hover),
            Stroke::new(1.0_f32, t::BORDER.lerp_to_gamma(t::ACCENT, hover)),
            t::TEXT,
        ),
    };
    let fade = if enabled { 1.0 } else { 0.4 };
    let p = ui.painter();
    p.rect_filled(rect, t::radius(), fill.gamma_multiply(fade));
    if stroke != Stroke::NONE {
        p.rect_stroke(rect, t::radius(), stroke, StrokeKind::Inside);
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::new(t::SIZE_BODY, t::semi_family()),
        text_color.gamma_multiply(fade),
    );
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, password: bool) -> Response {
    text_field_w(ui, value, hint, password, ui.available_width())
}

pub fn text_field_w(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    password: bool,
    width: f32,
) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(t::TEXT_DIM))
            .password(password)
            .margin(Margin::symmetric(14, 12))
            .desired_width((width - 28.0).max(40.0)),
    )
}

pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: &mut T,
    options: &[(T, String)],
) -> bool {
    let h = 42.0;
    // Explicit id base: several segmented controls on one page would otherwise clash.
    let base_id = ui.next_auto_id();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(rect, t::radius(), t::SURFACE_2);
    ui.painter().rect_stroke(
        rect,
        t::radius(),
        Stroke::new(1.0_f32, t::BORDER),
        StrokeKind::Inside,
    );
    let n = options.len().max(1) as f32;
    let seg_w = (rect.width() - 8.0) / n;
    let mut changed = false;
    for (i, (value, label)) in options.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + 4.0 + seg_w * i as f32, rect.top() + 4.0),
            vec2(seg_w, h - 8.0),
        );
        let resp = ui
            .interact(r, base_id.with(("seg", i)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let selected = *current == *value;
        let k = ui
            .ctx()
            .animate_value_with_time(resp.id, f32::from(selected), 0.15);
        let hover = f32::from(resp.hovered() && !selected) * 0.5;
        let fill = t::SURFACE_2
            .lerp_to_gamma(t::BORDER, hover)
            .lerp_to_gamma(t::ACCENT, k);
        ui.painter()
            .rect_filled(r, egui::CornerRadius::same(t::RADIUS - 3), fill);
        let color = t::TEXT_DIM.lerp_to_gamma(Color32::WHITE, k);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::new(t::SIZE_BODY - 1.0, t::semi_family()),
            color,
        );
        if resp.clicked() && !selected {
            *current = *value;
            changed = true;
        }
    }
    changed
}

pub struct ChipOut {
    pub clicked: bool,
    pub removed: bool,
}

/// Rounded topic chip. A `removable` chip shows a small cross with its own click target.
pub fn chip(ui: &mut Ui, label: &str, selected: bool, removable: bool) -> ChipOut {
    let font = FontId::proportional(t::SIZE_BODY - 1.5);
    let text_w = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), t::TEXT)
        .size()
        .x;
    let x_w = if removable { 22.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(text_w + 28.0 + x_w, 34.0), Sense::click());
    let k = ui
        .ctx()
        .animate_value_with_time(resp.id, f32::from(resp.hovered()), 0.12);
    let (fill, border, text) = if selected {
        (
            t::ACCENT.lerp_to_gamma(t::ACCENT_HOVER, k),
            t::ACCENT,
            Color32::WHITE,
        )
    } else {
        (
            t::SURFACE_2.lerp_to_gamma(t::BORDER, k * 0.6),
            t::BORDER.lerp_to_gamma(t::ACCENT, k),
            t::TEXT,
        )
    };
    let r = egui::CornerRadius::same(17);
    let p = ui.painter();
    p.rect_filled(rect, r, fill);
    p.rect_stroke(rect, r, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    p.text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        text,
    );
    let mut removed = false;
    if removable {
        let c = pos2(rect.right() - 17.0, rect.center().y);
        let hit = Rect::from_center_size(c, vec2(22.0, 22.0));
        let x_resp = ui
            .interact(hit, resp.id.with("remove"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let s = Stroke::new(
            1.6_f32,
            text.gamma_multiply(if x_resp.hovered() { 1.0 } else { 0.7 }),
        );
        let p = ui.painter();
        p.line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], s);
        p.line_segment([c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5)], s);
        removed = x_resp.clicked();
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    ChipOut {
        clicked: resp.clicked() && !removed,
        removed,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowIcon {
    Minimize,
    Maximize,
    Restore,
}

/// Square icon-only button for the custom title bar.
pub fn window_button(ui: &mut Ui, icon: WindowIcon, hint: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 34.0), Sense::click());
    let k = ui
        .ctx()
        .animate_value_with_time(resp.id, f32::from(resp.hovered()), 0.12);
    let color = t::TEXT_DIM.lerp_to_gamma(t::TEXT, k);
    let p = ui.painter();
    p.rect_filled(rect, t::radius(), t::SURFACE_2.gamma_multiply(k));
    let c = rect.center();
    let s = Stroke::new(1.5_f32, color);
    match icon {
        WindowIcon::Minimize => {
            p.line_segment([c + vec2(-5.0, 4.0), c + vec2(5.0, 4.0)], s);
        }
        WindowIcon::Maximize => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(11.0, 11.0)),
                2.0,
                s,
                StrokeKind::Inside,
            );
        }
        WindowIcon::Restore => {
            // Two overlapping squares: the back one is only partly visible.
            let back = Rect::from_center_size(c + vec2(2.0, -2.0), vec2(9.0, 9.0));
            let front = Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(9.0, 9.0));
            p.line_segment([back.left_top() + vec2(3.5, 0.0), back.right_top()], s);
            p.line_segment([back.right_top(), back.right_bottom() - vec2(0.0, 3.5)], s);
            p.rect_stroke(front, 2.0, s, StrokeKind::Inside);
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(hint)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GhostIcon {
    None,
    Close,
}

pub fn ghost_button(ui: &mut Ui, text: &str, icon: GhostIcon) -> Response {
    let font = FontId::new(t::SIZE_BODY - 1.5, t::semi_family());
    let text_w = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), t::TEXT)
        .size()
        .x;
    let icon_w = if icon == GhostIcon::None { 0.0 } else { 22.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(text_w + 28.0 + icon_w, 34.0), Sense::click());
    let k = ui
        .ctx()
        .animate_value_with_time(resp.id, f32::from(resp.hovered()), 0.12);
    // Closing gets a discreet coral hover.
    let (hover_fill, hover_text) = if icon == GhostIcon::Close {
        (t::tint(t::DANGER, 0.2), t::DANGER)
    } else {
        (t::SURFACE_2, t::TEXT)
    };
    let color = t::TEXT_DIM.lerp_to_gamma(hover_text, k);
    let p = ui.painter();
    p.rect_filled(rect, t::radius(), hover_fill.gamma_multiply(k));
    let mut x = rect.left() + 14.0;
    if icon == GhostIcon::Close {
        let c = pos2(x + 5.0, rect.center().y);
        let s = Stroke::new(1.8_f32, color);
        p.line_segment([c + vec2(-4.5, -4.5), c + vec2(4.5, 4.5)], s);
        p.line_segment([c + vec2(-4.5, 4.5), c + vec2(4.5, -4.5)], s);
        x += icon_w;
    }
    p.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let id = ui.next_auto_id();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let k = ui
        .ctx()
        .animate_value_with_time(id, f32::from(*value), 0.12);
    let boxr = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(22.0, 22.0));
    let r = egui::CornerRadius::same(6);
    let hover = f32::from(resp.hovered()) * 0.6;
    let p = ui.painter();
    p.rect_filled(boxr, r, t::SURFACE_2.lerp_to_gamma(t::ACCENT, k));
    p.rect_stroke(
        boxr,
        r,
        Stroke::new(
            1.5_f32,
            t::BORDER.lerp_to_gamma(t::ACCENT_HOVER, hover.max(k)),
        ),
        StrokeKind::Inside,
    );
    if *value {
        paint_check(p, boxr.center(), Color32::WHITE);
    }
    p.text(
        pos2(boxr.right() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(t::SIZE_BODY),
        t::TEXT,
    );
    let clicked = resp.clicked();
    if clicked {
        *value = !*value;
    }
    clicked
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(t::tint(color, 0.18))
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(t::semi(text, t::SIZE_CAPTION).color(color));
        });
}

pub fn progress_bar(ui: &mut Ui, fraction: f32, color: Color32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let f = ui
        .ctx()
        .animate_value_with_time(resp.id, fraction.clamp(0.0, 1.0), 0.25);
    let r = egui::CornerRadius::same(3);
    ui.painter().rect_filled(rect, r, t::SURFACE_2);
    let mut fill = rect;
    fill.set_width((rect.width() * f).max(if f > 0.0 { 6.0 } else { 0.0 }));
    ui.painter().rect_filled(fill, r, color);
}

// Not smoothed: the countdown value is already continuous.
pub fn timer_bar(ui: &mut Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let r = egui::CornerRadius::same(3);
    let f = fraction.clamp(0.0, 1.0);
    ui.painter().rect_filled(rect, r, t::SURFACE_2);
    let mut fill = rect;
    fill.set_width((rect.width() * f).max(if f > 0.0 { 6.0 } else { 0.0 }));
    ui.painter().rect_filled(fill, r, t::timer_color(f));
}

pub fn menu_card(ui: &mut Ui, title: &str, desc: &str, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), sense);
    let k = ui
        .ctx()
        .animate_value_with_time(resp.id, f32::from(resp.hovered() && enabled), 0.15);
    let p = ui.painter();
    p.rect_filled(
        rect,
        t::card_radius(),
        t::SURFACE.lerp_to_gamma(t::SURFACE_2, k),
    );
    p.rect_stroke(
        rect,
        t::card_radius(),
        Stroke::new(1.0_f32, t::BORDER.lerp_to_gamma(t::ACCENT, k)),
        StrokeKind::Inside,
    );
    let x = rect.left() + 24.0;
    p.text(
        pos2(x, rect.top() + 28.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(t::SIZE_SUBTITLE, t::semi_family()),
        t::TEXT,
    );
    p.text(
        pos2(x, rect.top() + 56.0),
        Align2::LEFT_CENTER,
        desc,
        FontId::proportional(t::SIZE_BODY - 1.0),
        t::TEXT_DIM,
    );
    let c = pos2(rect.right() - 28.0 + 4.0 * k, rect.center().y);
    let s = Stroke::new(2.0_f32, t::TEXT_DIM.lerp_to_gamma(t::ACCENT_HOVER, k));
    p.line_segment([c + vec2(-4.0, -7.0), c + vec2(3.0, 0.0)], s);
    p.line_segment([c + vec2(3.0, 0.0), c + vec2(-4.0, 7.0)], s);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn rank_pill(ui: &mut Ui, rank: u32) {
    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    let color = match rank {
        1 => t::GOLD,
        2 => t::SILVER,
        3 => t::BRONZE,
        _ => t::TEXT_DIM,
    };
    let medal = rank <= 3;
    let fill = if medal { color } else { t::SURFACE_2 };
    let text_color = if medal { t::BG } else { t::TEXT_DIM };
    ui.painter().circle_filled(rect.center(), 16.0, fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        rank.to_string(),
        FontId::new(t::SIZE_BODY - 1.0, t::semi_family()),
        text_color,
    );
}

pub fn avatar(ui: &mut Ui, name: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 16.0, t::tint(color, 0.25));
    let initial = name
        .chars()
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().collect());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::new(t::SIZE_BODY - 1.0, t::semi_family()),
        color,
    );
}

pub fn paint_check(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(2.4_f32, color);
    p.line_segment([c + vec2(-5.0, 0.5), c + vec2(-1.5, 4.0)], s);
    p.line_segment([c + vec2(-1.5, 4.0), c + vec2(5.5, -4.0)], s);
}

pub fn paint_cross(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(2.4_f32, color);
    p.line_segment([c + vec2(-4.5, -4.5), c + vec2(4.5, 4.5)], s);
    p.line_segment([c + vec2(-4.5, 4.5), c + vec2(4.5, -4.5)], s);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnswerState {
    Idle,
    Selected,
    Correct,
    Wrong,
    Dim,
}

pub fn answer_columns(width: f32) -> usize {
    if width >= 560.0 {
        2
    } else {
        1
    }
}

pub fn answer_grid(ui: &mut Ui, items: &[(&str, AnswerState)], interactive: bool) -> Option<usize> {
    let avail = ui.available_width();
    let cols = answer_columns(avail);
    let col_w = (avail - t::GAP * (cols as f32 - 1.0)) / cols as f32;
    let wrap = col_w - 16.0 - 32.0 - 14.0 - 32.0;
    let font = FontId::new(t::SIZE_BODY, egui::FontFamily::Proportional);
    let heights: Vec<f32> = items
        .iter()
        .map(|(text, _)| {
            let g = ui
                .painter()
                .layout(text.to_string(), font.clone(), t::TEXT, wrap.max(60.0));
            (g.size().y + 36.0).max(68.0)
        })
        .collect();
    let mut clicked = None;
    for (row, chunk) in items.chunks(cols).enumerate() {
        let row_h = heights[row * cols..row * cols + chunk.len()]
            .iter()
            .fold(0.0_f32, |a, b| a.max(*b));
        let (row_rect, _) = ui.allocate_exact_size(vec2(avail, row_h), Sense::hover());
        for (c, (text, state)) in chunk.iter().enumerate() {
            let i = row * cols + c;
            let rect = Rect::from_min_size(
                pos2(
                    row_rect.left() + c as f32 * (col_w + t::GAP),
                    row_rect.top(),
                ),
                vec2(col_w, row_h),
            );
            let resp = ui.interact(
                rect,
                ui.id().with(("answer", i)),
                if interactive {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            paint_answer(ui, rect, i, text, *state, interactive, &resp, &font, wrap);
            if interactive && resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if interactive && resp.clicked() {
                clicked = Some(i);
            }
        }
        ui.add_space(t::GAP - ui.spacing().item_spacing.y);
    }
    clicked
}

#[allow(clippy::too_many_arguments)]
fn paint_answer(
    ui: &Ui,
    rect: Rect,
    index: usize,
    text: &str,
    state: AnswerState,
    interactive: bool,
    resp: &Response,
    font: &FontId,
    wrap: f32,
) {
    let k =
        ui.ctx()
            .animate_value_with_time(resp.id, f32::from(resp.hovered() && interactive), 0.12);
    let (fill, border, text_color) = match state {
        AnswerState::Idle => (
            t::SURFACE.lerp_to_gamma(t::SURFACE_2, k),
            t::BORDER.lerp_to_gamma(t::ACCENT, k),
            t::TEXT,
        ),
        AnswerState::Selected => (t::tint(t::ACCENT, 0.22), t::ACCENT, t::TEXT),
        AnswerState::Correct => (t::tint(t::SUCCESS, 0.16), t::SUCCESS, t::TEXT),
        AnswerState::Wrong => (t::tint(t::DANGER, 0.16), t::DANGER, t::TEXT),
        AnswerState::Dim => (t::SURFACE, t::BORDER, t::TEXT_DIM),
    };
    let p = ui.painter();
    p.rect_filled(rect, t::card_radius(), fill);
    let width = if matches!(state, AnswerState::Correct | AnswerState::Wrong) {
        2.0_f32
    } else {
        1.0_f32
    };
    p.rect_stroke(
        rect,
        t::card_radius(),
        Stroke::new(width, border),
        StrokeKind::Inside,
    );
    let c = pos2(rect.left() + 16.0 + 16.0, rect.center().y);
    let letter = ["A", "B", "C", "D"].get(index).copied().unwrap_or("?");
    match state {
        AnswerState::Correct => {
            p.circle_filled(c, 16.0, t::SUCCESS);
            paint_check(p, c, t::BG);
        }
        AnswerState::Wrong => {
            p.circle_filled(c, 16.0, t::DANGER);
            paint_cross(p, c, t::BG);
        }
        _ => {
            let (bg, fg) = if state == AnswerState::Selected {
                (t::ACCENT, Color32::WHITE)
            } else {
                (t::SURFACE_2, t::TEXT_DIM)
            };
            p.circle_filled(c, 16.0, bg);
            p.circle_stroke(c, 16.0, Stroke::new(1.0_f32, t::BORDER));
            p.text(
                c,
                Align2::CENTER_CENTER,
                letter,
                FontId::new(t::SIZE_BODY - 1.0, t::semi_family()),
                fg,
            );
        }
    }
    let galley = p.layout(text.to_string(), font.clone(), text_color, wrap.max(60.0));
    let pos = pos2(c.x + 16.0 + 14.0, rect.center().y - galley.size().y / 2.0);
    p.galley(pos, galley, text_color);
    if interactive {
        p.text(
            pos2(rect.right() - 16.0, rect.top() + 14.0),
            Align2::RIGHT_CENTER,
            (index + 1).to_string(),
            FontId::proportional(t::SIZE_CAPTION),
            t::TEXT_DIM,
        );
    }
}

pub fn prompt_card(ui: &mut Ui, prompt: &str) {
    card(ui, |ui| {
        ui.label(t::semi(prompt, t::SIZE_SUBTITLE + 3.0));
    });
}

pub fn question_header(
    ui: &mut Ui,
    lang: quiz_core::Lang,
    index: usize,
    total: usize,
    right: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label(t::semi(
            tf(
                lang,
                Key::QuestionOf,
                &[&(index + 1).to_string(), &total.to_string()],
            ),
            t::SIZE_BODY,
        ));
        if let Some(r) = right {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(t::semi(r, t::SIZE_BODY).color(t::TEXT_DIM));
            });
        }
    });
    progress_bar(ui, (index + 1) as f32 / total.max(1) as f32, t::ACCENT);
    ui.add_space(t::GAP);
}

pub fn error_card(ui: &mut Ui, message: &str) {
    card_styled(ui, t::tint(t::DANGER, 0.08), t::DANGER, |ui| {
        ui.label(RichText::new(message).color(t::TEXT).size(t::SIZE_BODY));
    });
}

pub fn error_text(ui: &mut Ui, message: &str) {
    ui.label(
        RichText::new(message)
            .color(t::DANGER)
            .size(t::SIZE_BODY - 1.0),
    );
}

pub fn qr_code(ui: &mut Ui, side: usize, dark: &[bool], size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, Color32::WHITE);
    let quiet = 2.0;
    let cell = size / (side as f32 + 2.0 * quiet);
    for (i, _) in dark.iter().enumerate().filter(|(_, d)| **d) {
        let (x, y) = ((i % side) as f32, (i / side) as f32);
        let min = rect.min + vec2((x + quiet) * cell, (y + quiet) * cell);
        p.rect_filled(
            egui::Rect::from_min_size(min, vec2(cell + 0.5, cell + 0.5)),
            0.0,
            Color32::BLACK,
        );
    }
}

pub fn phone_icon(ui: &mut Ui, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(14.0, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_stroke(
        rect.shrink(1.0),
        3.0,
        egui::Stroke::new(1.6_f32, color),
        egui::StrokeKind::Inside,
    );
    p.line_segment(
        [
            rect.center_bottom() + vec2(-2.0, -4.0),
            rect.center_bottom() + vec2(2.0, -4.0),
        ],
        egui::Stroke::new(1.6_f32, color),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colonnes_selon_largeur() {
        assert_eq!(answer_columns(680.0), 2);
        assert_eq!(answer_columns(400.0), 1);
    }
}

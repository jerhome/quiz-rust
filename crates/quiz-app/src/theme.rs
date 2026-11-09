use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
    TextStyle,
};

pub const BG: Color32 = Color32::from_rgb(0x0E, 0x0F, 0x13);
pub const SURFACE: Color32 = Color32::from_rgb(0x16, 0x18, 0x1F);
pub const SURFACE_2: Color32 = Color32::from_rgb(0x1D, 0x20, 0x29);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x3A);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE9, 0xED);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9A, 0x9F, 0xAD);
pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x6C, 0xF2);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x8E, 0x80, 0xF5);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0x69, 0x59, 0xDB);
pub const SUCCESS: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);
pub const DANGER: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);
pub const WARNING: Color32 = Color32::from_rgb(0xF5, 0xB0, 0x4C);
pub const GOLD: Color32 = Color32::from_rgb(0xE9, 0xC4, 0x6A);
pub const SILVER: Color32 = Color32::from_rgb(0xB8, 0xBE, 0xCC);
pub const BRONZE: Color32 = Color32::from_rgb(0xD0, 0x8A, 0x5B);

pub const RADIUS: u8 = 12;
pub const CARD_RADIUS: u8 = 14;
pub const MAX_WIDTH: f32 = 680.0;
pub const GAP: f32 = 12.0;
pub const SECTION_GAP: f32 = 24.0;

pub const SIZE_DISPLAY: f32 = 44.0;
pub const SIZE_TITLE: f32 = 32.0;
pub const SIZE_SUBTITLE: f32 = 20.0;
pub const SIZE_BODY: f32 = 16.5;
pub const SIZE_CAPTION: f32 = 13.0;

pub fn radius() -> CornerRadius {
    CornerRadius::same(RADIUS)
}

pub fn card_radius() -> CornerRadius {
    CornerRadius::same(CARD_RADIUS)
}

pub fn semi_family() -> FontFamily {
    FontFamily::Name("semi".into())
}

pub fn semi(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, semi_family()))
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).size(SIZE_CAPTION + 1.5).color(TEXT_DIM)
}

pub fn tint(color: Color32, amount: f32) -> Color32 {
    SURFACE.lerp_to_gamma(color, amount)
}

pub fn timer_color(fraction_left: f32) -> Color32 {
    if fraction_left > 0.5 {
        ACCENT
    } else if fraction_left > 0.25 {
        ACCENT.lerp_to_gamma(WARNING, (0.5 - fraction_left) / 0.25)
    } else {
        WARNING.lerp_to_gamma(DANGER, (0.25 - fraction_left) / 0.25)
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let regular = include_bytes!("../assets/fonts/Inter-Regular.ttf");
    let semibold = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
    fonts
        .font_data
        .insert("Inter".into(), Arc::new(FontData::from_static(regular)));
    fonts.font_data.insert(
        "Inter-Semi".into(),
        Arc::new(FontData::from_static(semibold)),
    );
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut proportional = vec!["Inter".to_string()];
    proportional.extend(fallback.iter().cloned());
    let mut semi_stack = vec!["Inter-Semi".to_string()];
    semi_stack.extend(fallback);
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    fonts.families.insert(semi_family(), semi_stack);
    ctx.set_fonts(fonts);
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(SIZE_TITLE, semi_family())),
            (TextStyle::Body, FontId::proportional(SIZE_BODY)),
            (TextStyle::Button, FontId::new(SIZE_BODY, semi_family())),
            (TextStyle::Small, FontId::proportional(SIZE_CAPTION)),
            (TextStyle::Monospace, FontId::monospace(15.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(GAP, GAP);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.spacing.interact_size.y = 40.0;
        style.spacing.slider_rail_height = 6.0;
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.extreme_bg_color = SURFACE_2;
        v.faint_bg_color = SURFACE;
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.5_f32, ACCENT_HOVER);
        v.slider_trailing_fill = true;
        v.widgets.noninteractive.bg_fill = SURFACE;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = radius();
        }
        v.widgets.noninteractive.corner_radius = radius();
        v.widgets.inactive.bg_fill = BORDER;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.widgets.inactive.weak_bg_fill = SURFACE_2;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        v.widgets.hovered.bg_fill = SURFACE_2;
        v.widgets.hovered.weak_bg_fill = SURFACE_2;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        v.widgets.active.bg_fill = ACCENT_PRESSED;
        v.widgets.active.weak_bg_fill = ACCENT_PRESSED;
        v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleur_du_compte_a_rebours() {
        assert_eq!(timer_color(1.0), ACCENT);
        assert_eq!(timer_color(0.0), DANGER);
        assert_ne!(timer_color(0.4), ACCENT);
    }
}

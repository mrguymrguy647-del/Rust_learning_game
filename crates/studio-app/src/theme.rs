//! Visual theme: one consistent palette, spacing and type scale.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use studio_core::settings::Settings;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub info: Color32,
    pub code_bg: Color32,
}

impl Palette {
    pub const DARK: Palette = Palette {
        bg: Color32::from_rgb(0x12, 0x14, 0x1a),
        panel: Color32::from_rgb(0x18, 0x1b, 0x23),
        card: Color32::from_rgb(0x1f, 0x23, 0x2d),
        card_hover: Color32::from_rgb(0x28, 0x2d, 0x3a),
        border: Color32::from_rgb(0x2e, 0x34, 0x44),
        text: Color32::from_rgb(0xe6, 0xe8, 0xee),
        dim: Color32::from_rgb(0x9a, 0xa3, 0xb5),
        accent: Color32::from_rgb(0xff, 0x7a, 0x3d),
        accent_text: Color32::from_rgb(0x1a, 0x10, 0x0a),
        good: Color32::from_rgb(0x5f, 0xd2, 0x8a),
        warn: Color32::from_rgb(0xf2, 0xc1, 0x4e),
        bad: Color32::from_rgb(0xf2, 0x6b, 0x6b),
        info: Color32::from_rgb(0x6a, 0xa9, 0xff),
        code_bg: Color32::from_rgb(0x0f, 0x11, 0x16),
    };

    pub const LIGHT: Palette = Palette {
        bg: Color32::from_rgb(0xf4, 0xf1, 0xec),
        panel: Color32::from_rgb(0xeb, 0xe7, 0xe0),
        card: Color32::from_rgb(0xff, 0xff, 0xff),
        card_hover: Color32::from_rgb(0xf6, 0xf2, 0xea),
        border: Color32::from_rgb(0xd6, 0xd0, 0xc6),
        text: Color32::from_rgb(0x23, 0x26, 0x2d),
        dim: Color32::from_rgb(0x6a, 0x70, 0x80),
        accent: Color32::from_rgb(0xd9, 0x53, 0x1e),
        accent_text: Color32::WHITE,
        good: Color32::from_rgb(0x1f, 0x8f, 0x55),
        warn: Color32::from_rgb(0xa8, 0x76, 0x00),
        bad: Color32::from_rgb(0xc6, 0x3b, 0x3b),
        info: Color32::from_rgb(0x2a, 0x6f, 0xd6),
        code_bg: Color32::from_rgb(0xfa, 0xf8, 0xf4),
    };

    pub fn of(ui: &egui::Ui) -> Palette {
        Palette::from_dark(ui.visuals().dark_mode)
    }

    pub fn from_dark(dark: bool) -> Palette {
        if dark {
            Palette::DARK
        } else {
            Palette::LIGHT
        }
    }
}

/// egui's default proportional font lacks arrows and geometric shapes (▲ ▼ → ●), but the bundled
/// monospace font has them: use it as a fallback so those icons render everywhere.
fn install_fonts(ctx: &egui::Context) {
    let marker = egui::Id::new("rst_fonts_installed");
    if ctx.data(|d| d.get_temp::<bool>(marker)).unwrap_or(false) {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| d.insert_temp(marker, true));
}

/// Install visuals, fonts sizes and zoom for the given settings.
pub fn apply(ctx: &egui::Context, settings: &Settings) {
    install_fonts(ctx);
    let pal = Palette::from_dark(settings.dark_mode);
    let mut visuals = if settings.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.panel_fill = pal.panel;
    visuals.window_fill = pal.card;
    visuals.extreme_bg_color = pal.code_bg;
    visuals.faint_bg_color = pal.card;
    visuals.override_text_color = Some(pal.text);
    visuals.hyperlink_color = pal.info;
    visuals.selection.bg_fill = pal.accent.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, pal.accent);
    visuals.window_stroke = Stroke::new(1.0, pal.border);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    let radius = CornerRadius::same(7);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    visuals.widgets.inactive.bg_fill = pal.card_hover;
    visuals.widgets.inactive.weak_bg_fill = pal.card_hover;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.border);
    visuals.widgets.hovered.bg_fill = pal.card_hover;
    visuals.widgets.hovered.weak_bg_fill = pal.card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.accent);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, pal.accent);

    let theme = if settings.dark_mode { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(if settings.dark_mode {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.set_visuals_of(theme, visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        style.spacing.window_margin = egui::Margin::same(14);
        // Solid scroll bars reserve their own space instead of overlapping buttons at the right edge.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(settings.editor_font_size, FontFamily::Monospace)),
        ]
        .into();
    });
    ctx.set_zoom_factor(settings.ui_scale);
}

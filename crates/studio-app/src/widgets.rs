//! Reusable UI building blocks.

use eframe::egui::{self, Color32, RichText, Stroke, Ui};

use crate::theme::Palette;

/// A rounded panel with a border; the basic container of every screen.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let pal = Palette::of(ui);
    egui::Frame::new()
        .fill(pal.card)
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(14))
        .show(ui, add)
}

/// Section heading with a thin underline.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong().size(17.0));
    ui.separator();
}

pub fn dim(ui: &mut Ui, text: impl Into<String>) {
    let pal = Palette::of(ui);
    ui.label(RichText::new(text.into()).color(pal.dim));
}

/// Primary (accent coloured) button.
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    let pal = Palette::of(ui);
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(pal.accent_text))
            .fill(pal.accent)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

/// A small label chip: `Money  $1,200`.
pub fn chip(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    let pal = Palette::of(ui);
    egui::Frame::new()
        .fill(pal.card)
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(label).small().color(pal.dim));
                ui.label(RichText::new(value).strong().color(color));
            });
        });
}

/// A horizontal bar filled to `fraction`, with optional centred text. Draws cleanly at 0 %.
pub fn bar(
    ui: &mut Ui,
    fraction: f32,
    color: Color32,
    text: Option<&str>,
    width: f32,
    height: f32,
) -> egui::Response {
    let pal = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let rounding = (height / 2.0).round() as u8;
    painter.rect_filled(rect, rounding, pal.code_bg);
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.0 {
        let fill_w = (rect.width() * fraction).max(height * 0.6).min(rect.width());
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        painter.rect_filled(fill, rounding, color);
    }
    painter.rect_stroke(rect, rounding, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
    if let Some(text) = text {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(12.5),
            pal.text,
        );
    }
    response
}

/// Left-aligned label in a fixed-width column (so rows line up in lists).
pub fn label_fixed(ui: &mut Ui, width: f32, text: impl Into<egui::WidgetText>) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(text).truncate());
        },
    );
}

/// Thin labelled progress bar.
pub fn meter(ui: &mut Ui, label: &str, fraction: f32, color: Color32) {
    ui.horizontal(|ui| {
        label_fixed(ui, 150.0, label);
        let text = format!("{:.0}%", fraction.clamp(0.0, 1.0) * 100.0);
        bar(ui, fraction, color, Some(&text), ui.available_width().min(260.0), 18.0);
    });
}

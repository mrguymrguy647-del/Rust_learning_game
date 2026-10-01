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

/// Thin labelled progress bar.
pub fn meter(ui: &mut Ui, label: &str, fraction: f32, color: Color32) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(150.0, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.add(egui::Label::new(label).truncate()),
        );
        ui.add(
            egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                .desired_width(ui.available_width().min(260.0))
                .fill(color)
                .text(format!("{:.0}%", fraction.clamp(0.0, 1.0) * 100.0)),
        );
    });
}

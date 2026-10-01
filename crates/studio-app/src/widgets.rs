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
        let fill_w = (rect.width() * fraction).min(rect.width());
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

/// Four-step phase indicator with the current phase highlighted and a progress bar below.
pub fn phase_stepper(ui: &mut Ui, current: studio_core::sim::Phase, fraction: f32) {
    let pal = Palette::of(ui);
    ui.horizontal(|ui| {
        for phase in studio_core::sim::Phase::ALL {
            let done = phase < current;
            let active = phase == current;
            let (fill, text_color) = if active {
                (pal.accent, pal.accent_text)
            } else if done {
                (pal.good.gamma_multiply(0.35), pal.text)
            } else {
                (pal.card_hover, pal.dim)
            };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(12)
                .inner_margin(egui::Margin::symmetric(12, 4))
                .show(ui, |ui| {
                    let mark = if done { "✔ " } else { "" };
                    ui.label(RichText::new(format!("{mark}{}", phase.label())).color(text_color).strong());
                });
        }
    });
    bar(
        ui,
        fraction,
        pal.accent,
        Some(&format!("{:.0}%", fraction * 100.0)),
        ui.available_width().min(640.0),
        18.0,
    );
}

/// A tiny line chart of `values` (e.g. weekly sales).
pub fn sparkline(ui: &mut Ui, values: &[f32], size: egui::Vec2, color: Color32) {
    let pal = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4, pal.code_bg);
    if values.len() < 2 {
        return;
    }
    let max = values.iter().copied().fold(1.0f32, f32::max);
    let n = values.len() as f32 - 1.0;
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                rect.left() + 3.0 + (rect.width() - 6.0) * i as f32 / n,
                rect.bottom() - 3.0 - (rect.height() - 6.0) * (v / max),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
}

/// A review score (0–10) in a coloured pill.
pub fn score_badge(ui: &mut Ui, score: f32) {
    let pal = Palette::of(ui);
    let color = if score >= 8.0 {
        pal.good
    } else if score >= 6.0 {
        pal.info
    } else if score >= 4.0 {
        pal.warn
    } else {
        pal.bad
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.25))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("{score:.1}")).strong().size(17.0).color(color));
        });
}

/// Colour for a 0–100 metascore.
pub fn meta_color(pal: &Palette, meta: f32) -> Color32 {
    if meta >= 80.0 {
        pal.good
    } else if meta >= 60.0 {
        pal.info
    } else if meta >= 40.0 {
        pal.warn
    } else {
        pal.bad
    }
}

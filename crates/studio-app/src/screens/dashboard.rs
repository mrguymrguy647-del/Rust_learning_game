//! The studio overview.

use eframe::egui::{self, RichText};
use studio_core::fmt;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let Some(game) = &app.game else {
        return;
    };
    ui.heading(format!("Welcome, {}", game.studio.founder));
    widgets::dim(ui, "Press ▶ in the top bar to let time pass. Challenges pause the clock.");
    ui.add_space(8.0);

    let tier = app.content.tier(game.studio.tier);
    ui.columns(2, |cols| {
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Studio");
            ui.label(RichText::new(&game.studio.name).strong().size(18.0));
            if let Some(t) = tier {
                ui.label(RichText::new(&t.name).color(pal.accent));
                widgets::dim(ui, &t.description);
                ui.add_space(4.0);
                ui.label(format!("Rent: {} / week", fmt::money(t.rent)));
                ui.label(format!("Staff slots: {}", t.staff_slots));
            }
            ui.label(format!("Cash: {}", fmt::money(game.studio.money)));
        });
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Rust curriculum");
            for topic in app.content.topics.iter().take(6) {
                widgets::meter(ui, &topic.name, 0.0, pal.accent);
            }
            widgets::dim(ui, "…and more. See the Skills screen.");
        });
    });
}

//! Settings screen: difficulty, hint costs, sandbox timeout, editor font size, theme.

use eframe::egui::{self, RichText};
use studio_core::settings::Difficulty;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    ui.heading("Settings");
    ui.add_space(6.0);

    let mut changed = false;

    widgets::card(ui, |ui| {
        widgets::section(ui, "Economy");
        ui.horizontal(|ui| {
            ui.label("Difficulty");
            let mut diff = app.game.as_ref().map(|g| g.difficulty).unwrap_or(app.settings.default_difficulty);
            let before = diff;
            for d in Difficulty::ALL {
                ui.selectable_value(&mut diff, d, d.label());
            }
            if diff != before {
                app.settings.default_difficulty = diff;
                if let Some(game) = app.game.as_mut() {
                    game.difficulty = diff;
                }
                changed = true;
            }
        });
        widgets::dim(ui, "Changes revenue and costs only. Challenges and their rewards stay the same.");
    });

    ui.add_space(8.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Challenges");
        changed |= ui
            .add(
                egui::Slider::new(&mut app.settings.hint_cost_multiplier, 0.0..=3.0)
                    .text("Hint cost multiplier"),
            )
            .changed();
        widgets::dim(ui, "0 makes hints free. 1 is the default price. Practice mode hints are always free.");
        let mut timeout = app.settings.sandbox_timeout_secs as f64;
        if ui.add(egui::Slider::new(&mut timeout, 5.0..=120.0).text("Test timeout (seconds)")).changed() {
            app.settings.sandbox_timeout_secs = timeout as u64;
            changed = true;
        }
        widgets::dim(ui, "Your code is killed if its tests run longer than this (infinite loops!).");
        let mut failures = app.settings.contractor_after_failures as f64;
        if ui
            .add(egui::Slider::new(&mut failures, 1.0..=10.0).text("Failed attempts before contractor"))
            .changed()
        {
            app.settings.contractor_after_failures = failures as u32;
            changed = true;
        }
    });

    ui.add_space(8.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Appearance");
        changed |= ui.checkbox(&mut app.settings.dark_mode, "Dark theme").changed();
        changed |= ui
            .add(egui::Slider::new(&mut app.settings.editor_font_size, 10.0..=28.0).text("Editor font size"))
            .changed();
        changed |=
            ui.add(egui::Slider::new(&mut app.settings.ui_scale, 0.8..=1.8).text("UI scale")).changed();
        ui.add_space(4.0);
        ui.label(RichText::new("fn main() { println!(\"Hello, Ferris!\"); }").monospace().color(pal.accent));
    });

    ui.add_space(8.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Saving");
        changed |=
            ui.checkbox(&mut app.settings.autosave, "Autosave every few weeks and when leaving").changed();
    });

    ui.add_space(8.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Tutorial");
        widgets::dim(ui, "A short tour of the tycoon screens, and a tour of the code editor in your next coding challenge.");
        if ui.button("Replay the tutorial").clicked() {
            crate::tutorial::replay(app);
        }
    });

    if changed {
        app.settings = app.settings.clone().sanitized();
        app.mark_settings_dirty();
    }
}

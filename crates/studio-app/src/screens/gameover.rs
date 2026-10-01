//! The bankruptcy screen.

use eframe::egui::{self, RichText};
use studio_core::fmt;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let Some(over) = game.game_over.clone() else {
        return;
    };
    let pal = Palette::from_dark(app.settings.dark_mode);
    let (name, revenue, games, solved, level) = (
        game.studio.name.clone(),
        game.total_revenue(),
        game.games.len(),
        game.progress.solved.len(),
        game.progress.level(),
    );
    let mut leave = false;
    egui::Window::new("The studio has closed")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Bankrupt").size(30.0).strong().color(pal.bad));
            ui.label(&over.reason);
            ui.add_space(8.0);
            ui.label(format!("{name} survived {} and released {games} game(s) earning {} in total.", studio_core::sim::GameDate(over.week), fmt::money(revenue)));
            ui.label(format!("You solved {solved} Rust challenges and reached level {level}. That knowledge stays with you."));
            widgets::dim(ui, "Load an earlier save to try again, or start a fresh studio from the main menu.");
            ui.add_space(8.0);
            if widgets::primary_button(ui, "Back to the main menu").clicked() {
                leave = true;
            }
        });
    if leave {
        app.exit_to_title();
    }
}

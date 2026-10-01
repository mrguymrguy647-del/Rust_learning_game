//! The review window shown after a release (and from the Library).

use eframe::egui::{self, RichText};
use studio_core::fmt;
use studio_core::sim::Category;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show_popup(app: &mut App, ctx: &egui::Context) {
    let Some(idx) = app.review_popup else {
        return;
    };
    let Some(game) = app.game.as_ref().and_then(|g| g.games.get(idx)).cloned() else {
        app.review_popup = None;
        return;
    };
    let pal = Palette::from_dark(app.settings.dark_mode);
    let genre =
        app.content.genres.iter().find(|g| g.id == game.genre).map(|g| g.name.clone()).unwrap_or_default();
    let mut open = true;
    egui::Window::new(format!("Reviews — {}", game.name))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(640.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:.0}", game.metascore))
                        .size(44.0)
                        .strong()
                        .color(widgets::meta_color(&pal, game.metascore)),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("Metascore").color(pal.dim));
                    ui.label(format!("{genre} · {} · bugs ≈ {:.0}%", game.size.label(), game.bugs));
                });
            });
            ui.add_space(6.0);
            for r in &game.reviews {
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::score_badge(ui, r.score);
                        ui.label(RichText::new(&r.outlet).strong());
                    });
                    ui.label(RichText::new(format!("“{}”", r.quote)).italics());
                });
                ui.add_space(2.0);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("What the critics saw").strong());
            for c in Category::ALL {
                widgets::meter(
                    ui,
                    c.label(),
                    game.categories.get(c) / 100.0,
                    widgets::meta_color(&pal, game.categories.get(c)),
                );
            }
            ui.add_space(4.0);
            if game.age(app.game.as_ref().map(|g| g.date.week()).unwrap_or(0)) == 0 {
                widgets::dim(
                    ui,
                    format!(
                        "Sales start next week. Expected launch week: about {} copies.",
                        fmt::compact(game.launch_units as i64)
                    ),
                );
            } else {
                widgets::dim(
                    ui,
                    format!(
                        "{} copies sold, {} revenue so far.",
                        fmt::compact(game.units_total as i64),
                        fmt::money(game.revenue_total)
                    ),
                );
            }
        });
    if !open {
        app.review_popup = None;
    }
}

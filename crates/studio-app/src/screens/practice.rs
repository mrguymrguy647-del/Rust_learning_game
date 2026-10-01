//! Practice mode: replay any solved challenge. Nothing here touches the economy or progress.

use eframe::egui::{self, RichText};
use studio_core::data::ChallengeKind;
use studio_core::sim::ChallengeContext;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let content = app.content.clone();
    let solved: Vec<String> = game.progress.solved.keys().cloned().collect();
    let mut start: Option<String> = None;

    ui.heading("Practice");
    widgets::dim(ui, "Replay any challenge you have already solved. Hints are free and nothing you do here changes your studio.");
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("Show:");
        let filter = &mut app.practice_filter;
        if ui.selectable_label(filter.is_none(), "All kinds").clicked() {
            *filter = None;
        }
        for kind in ChallengeKind::ALL {
            if ui.selectable_label(*filter == Some(kind), kind.label()).clicked() {
                *filter = Some(kind);
            }
        }
    });
    ui.add_space(6.0);

    if solved.is_empty() {
        widgets::card(ui, |ui| {
            ui.label("You have not solved any challenges yet. Head to Skills to study your first one, or build engine modules to unlock more.");
        });
        return;
    }

    for topic in &content.topics {
        let rows: Vec<_> = content
            .challenges_in_topic(&topic.id)
            .filter(|c| solved.contains(&c.id))
            .filter(|c| app.practice_filter.is_none_or(|k| k == c.kind))
            .collect();
        if rows.is_empty() {
            continue;
        }
        ui.label(RichText::new(format!("{} — {}", topic.name, topic.feature_name)).strong().size(16.0));
        for c in rows {
            ui.horizontal(|ui| {
                ui.label(RichText::new("✔").color(pal.good));
                widgets::label_fixed(ui, 240.0, RichText::new(&c.title).strong());
                ui.label(RichText::new(c.kind.label()).color(pal.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Practice").clicked() {
                        start = Some(c.id.clone());
                    }
                });
            });
        }
        ui.add_space(6.0);
    }
    if let Some(id) = start {
        app.open_challenge(&id, ChallengeContext::Practice);
    }
}

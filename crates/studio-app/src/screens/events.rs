//! The event window: a decision the player has to make before time continues.

use eframe::egui::{self, RichText};
use studio_core::data::{ChoiceAction, EventKind};
use studio_core::fmt;
use studio_core::sim::events::EventOutcome;

use crate::app::{App, ToastKind};
use crate::theme::Palette;

pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.challenge.is_some() {
        return;
    }
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let Some(active) = game.events.pending.clone() else {
        return;
    };
    let Some(def) = content.event(&active.def_id).cloned() else {
        return;
    };
    let pal = Palette::from_dark(app.settings.dark_mode);
    let text = game.event_text(&def.text, active.game_id);
    let money = game.studio.money;
    let choices: Vec<_> = def
        .choices
        .iter()
        .map(|c| {
            (
                game.event_text(&c.label, active.game_id),
                game.event_text(&c.detail, active.game_id),
                c.cost,
                c.action,
            )
        })
        .collect();
    let mut chosen: Option<usize> = None;

    let accent = match def.kind {
        EventKind::Hotfix => pal.bad,
        EventKind::GameJam => pal.info,
        EventKind::Publisher => pal.good,
        EventKind::Conference => pal.accent,
        EventKind::Crunch => pal.warn,
        EventKind::Decision => pal.dim,
    };
    let kind_label = match def.kind {
        EventKind::Hotfix => "CRISIS",
        EventKind::GameJam => "GAME JAM",
        EventKind::Publisher => "PUBLISHER",
        EventKind::Conference => "CONFERENCE",
        EventKind::Crunch => "DECISION",
        EventKind::Decision => "EVENT",
    };

    egui::Window::new(&def.title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(kind_label).small().strong().color(accent));
            ui.add_space(2.0);
            ui.label(RichText::new(&text).size(16.0));
            ui.add_space(8.0);
            for (i, (label, detail, cost, action)) in choices.iter().enumerate() {
                let affordable = money >= *cost;
                let button = egui::Button::new(RichText::new(label).strong())
                    .min_size(egui::vec2(ui.available_width(), 30.0));
                let response = ui.add_enabled(affordable, button);
                if response.clicked() {
                    chosen = Some(i);
                }
                if !detail.is_empty() {
                    ui.label(RichText::new(detail).small().color(pal.dim));
                }
                if !affordable {
                    ui.label(
                        RichText::new(format!("You need {} for this.", fmt::money(*cost)))
                            .small()
                            .color(pal.warn),
                    );
                }
                if matches!(action, ChoiceAction::StartHotfix | ChoiceAction::StartJam) {
                    ui.label(RichText::new("Opens a Rust challenge.").small().color(pal.info));
                }
                ui.add_space(4.0);
            }
        });

    if let Some(i) = chosen {
        let result = match app.game.as_mut() {
            Some(g) => g.resolve_event(&content, i),
            None => return,
        };
        match result {
            Ok(EventOutcome::Done(msg)) => {
                app.toast(ToastKind::Info, msg);
                app.autosave();
            }
            Ok(EventOutcome::Challenge(_)) => {
                if let Some(a) = app.game.as_ref().and_then(|g| g.pending_attempt.clone()) {
                    app.open_challenge(&a.challenge_id, a.context);
                }
            }
            Err(e) => app.toast(ToastKind::Warn, e),
        }
    }
}

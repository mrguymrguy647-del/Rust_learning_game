//! Rust Codex: the in-game handbook. Entries unlock as the matching challenge is solved.

use eframe::egui::{self, RichText};
use egui_extras::syntax_highlighting::{code_view_ui, CodeTheme};
use studio_core::data::CodexEntry;
use studio_core::sim::ChallengeContext;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

/// Width of the entry list on the left.
const LIST_WIDTH: f32 = 290.0;

fn matches_search(entry: &CodexEntry, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle = needle.to_lowercase();
    entry.title.to_lowercase().contains(&needle)
        || entry.summary.to_lowercase().contains(&needle)
        || entry.body.to_lowercase().contains(&needle)
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let progress = game.progress.clone();

    ui.heading("Rust Codex");
    widgets::dim(
        ui,
        "Your personal Rust handbook. Each entry unlocks when you solve the challenge that teaches it.",
    );
    ui.add_space(4.0);
    let unlocked = content.codex.iter().filter(|e| progress.codex_unlocked(e)).count();
    ui.horizontal(|ui| {
        widgets::chip(ui, "Unlocked", &format!("{unlocked}/{}", content.codex.len()), pal.good);
        let fresh = progress.codex_new_count(&content.codex);
        if fresh > 0 {
            widgets::chip(ui, "New", &fresh.to_string(), pal.info);
        }
        ui.add_space(8.0);
        ui.label("Search:");
        ui.add(egui::TextEdit::singleline(&mut app.codex_search).desired_width(220.0));
        if !app.codex_search.is_empty() && ui.small_button("✖").clicked() {
            app.codex_search.clear();
        }
    });
    ui.add_space(6.0);

    let needle = app.codex_search.trim().to_string();
    let mut select: Option<String> = None;
    let mut replay: Option<String> = None;

    ui.horizontal_top(|ui| {
        // ------------------------------------------------------------------ list
        ui.allocate_ui_with_layout(
            egui::vec2(LIST_WIDTH, 10.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(LIST_WIDTH);
                for topic in &content.topics {
                    let entries: Vec<&CodexEntry> = content
                        .codex
                        .iter()
                        .filter(|e| e.topic == topic.id && matches_search(e, &needle))
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    let open_now =
                        app.codex_selected.as_ref().is_some_and(|sel| entries.iter().any(|e| &e.id == sel));
                    let mut header = egui::CollapsingHeader::new(
                        RichText::new(format!("{}. {}", topic.order, topic.name)).strong(),
                    )
                    .id_salt(("codex_topic", &topic.id));
                    if !needle.is_empty() || open_now {
                        header = header.open(Some(true));
                    }
                    header.show(ui, |ui| {
                        for e in entries {
                            let open = progress.codex_unlocked(e);
                            let fresh = open && !progress.codex_seen.contains(&e.id);
                            let selected = app.codex_selected.as_deref() == Some(e.id.as_str());
                            let mut text = RichText::new(if open {
                                e.title.clone()
                            } else {
                                format!("🔒 {}", e.title)
                            });
                            if !open {
                                text = text.color(pal.dim);
                            } else if fresh {
                                text = text.strong();
                            }
                            ui.horizontal(|ui| {
                                if ui.selectable_label(selected, text).clicked() {
                                    select = Some(e.id.clone());
                                }
                                if fresh {
                                    ui.label(RichText::new("NEW").small().strong().color(pal.info));
                                }
                            });
                        }
                    });
                }
            },
        );
        ui.add_space(10.0);

        // --------------------------------------------------------------- reading pane
        ui.vertical(|ui| {
            let selected =
                app.codex_selected.as_ref().and_then(|id| content.codex.iter().find(|e| &e.id == id));
            let Some(entry) = selected else {
                widgets::card(ui, |ui| {
                    ui.label("Pick an entry on the left to read it.");
                    widgets::dim(ui, "Locked entries show which challenge unlocks them.");
                });
                return;
            };
            let topic_name =
                content.topic(&entry.topic).map(|t| t.name.clone()).unwrap_or_else(|| entry.topic.clone());
            widgets::card(ui, |ui| {
                ui.label(RichText::new(&entry.title).size(22.0).strong().color(pal.accent));
                ui.label(RichText::new(format!("Topic: {topic_name}")).small().color(pal.dim));
                ui.add_space(4.0);

                if !progress.codex_unlocked(entry) {
                    let challenge = entry.unlock_challenge.as_deref().and_then(|id| content.challenge(id));
                    widgets::markup_styled(ui, &entry.summary, true, Some(pal.dim));
                    ui.add_space(8.0);
                    ui.label(RichText::new("🔒 Locked").strong().color(pal.warn));
                    if let Some(c) = challenge {
                        ui.label(format!("Solve “{}” to unlock this entry.", c.title));
                    }
                    return;
                }

                widgets::markup_styled(ui, &entry.summary, true, None);
                ui.add_space(8.0);
                for paragraph in entry.body.split("\n\n") {
                    widgets::markup(ui, paragraph.trim());
                    ui.add_space(6.0);
                }
                if !entry.example.trim().is_empty() {
                    widgets::section(ui, "Example");
                    let theme = CodeTheme::from_style(ui.style());
                    code_view_ui(ui, &theme, entry.example.trim_end(), "rs");
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if !entry.book_url.is_empty() {
                        ui.hyperlink_to("Read more ↗", &entry.book_url);
                    }
                    if let Some(id) = &entry.unlock_challenge {
                        if let Some(c) = content.challenge(id) {
                            ui.label(
                                RichText::new(format!("Learned in: {}", c.title)).small().color(pal.dim),
                            );
                            if ui.button("Replay challenge").clicked() {
                                replay = Some(id.clone());
                            }
                        }
                    }
                });
            });
        });
    });

    if let Some(id) = select {
        if let Some(entry) = content.codex.iter().find(|e| e.id == id) {
            if progress.codex_unlocked(entry) {
                if let Some(g) = app.game.as_mut() {
                    g.progress.mark_codex_seen(&id);
                }
            }
        }
        app.codex_selected = Some(id);
    }
    if let Some(id) = replay {
        app.open_challenge(&id, ChallengeContext::Practice);
    }
}

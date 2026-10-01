//! The engine tree: build modules, see what they unlock, and research with challenges.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use studio_core::data::{Challenge, ContentLibrary, EngineModuleDef};
use studio_core::fmt;
use studio_core::sim::engine::Blocker;
use studio_core::sim::research::describe_blocker;
use studio_core::sim::{Availability, Category, ChallengeContext, GameState};

use crate::app::{App, ToastKind};
use crate::theme::Palette;
use crate::widgets;

/// Depth of each module in the dependency tree (roots = 0).
fn depths(content: &ContentLibrary) -> HashMap<String, usize> {
    let mut depth: HashMap<String, usize> = HashMap::new();
    // Modules are acyclic (validated), so repeated relaxation terminates quickly.
    for _ in 0..content.engine_modules.len() + 1 {
        for m in &content.engine_modules {
            let d =
                m.requires_modules.iter().map(|r| depth.get(r).copied().unwrap_or(0) + 1).max().unwrap_or(0);
            let entry = depth.entry(m.id.clone()).or_insert(0);
            if d > *entry {
                *entry = d;
            }
        }
    }
    depth
}

/// Available challenges in the topics a module still needs.
fn lab_challenges<'a>(
    content: &'a ContentLibrary,
    game: &GameState,
    m: &EngineModuleDef,
) -> Vec<&'a Challenge> {
    let mut out = Vec::new();
    let mut topics: Vec<&String> = m.required_topic_solves.iter().map(|(t, _)| t).collect();
    if topics.is_empty() {
        topics.push(&m.topic);
    }
    for topic in topics {
        for c in content.challenges_in_topic(topic) {
            if game.progress.availability(content, c) == Availability::Available {
                out.push(c);
            }
        }
    }
    out
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game = game.clone();
    let mut build: Option<String> = None;
    let mut study: Option<(String, String)> = None;

    ui.heading("Engine");
    widgets::dim(ui, "Modules make your games better and unlock genres, sizes and platforms. Each one needs solved challenges, research points and money.");
    ui.add_space(6.0);

    ui.horizontal_wrapped(|ui| {
        widgets::chip(ui, "Research points", &game.studio.research_points.to_string(), pal.accent);
        widgets::chip(ui, "Per week", &format!("+{:.1}", game.weekly_research(&content)), pal.info);
        widgets::chip(
            ui,
            "Built",
            &format!("{}/{}", game.engine.built.len(), content.engine_modules.len()),
            pal.good,
        );
        widgets::chip(
            ui,
            "Cash",
            &fmt::money(game.studio.money),
            if game.studio.money < 0 { pal.bad } else { pal.good },
        );
    });
    if let Some(b) = &game.engine.building {
        if let Some(m) = content.module(&b.module) {
            ui.add_space(4.0);
            let done = m.build_weeks.saturating_sub(b.weeks_left);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Building {}", m.name)).strong());
                widgets::bar(
                    ui,
                    done as f32 / m.build_weeks.max(1) as f32,
                    pal.accent,
                    Some(&format!("{} week(s) left", b.weeks_left)),
                    260.0,
                    18.0,
                );
            });
        }
    }
    widgets::dim(ui, "Programmers generate research points every week. Solving a challenge in the Engine lab pays the most; Study pays half.");
    ui.add_space(8.0);

    let depth = depths(&content);
    let max_depth = depth.values().copied().max().unwrap_or(0);
    for layer in 0..=max_depth {
        let modules: Vec<&EngineModuleDef> = content
            .engine_modules
            .iter()
            .filter(|m| depth.get(&m.id).copied().unwrap_or(0) == layer)
            .collect();
        if modules.is_empty() {
            continue;
        }
        ui.label(
            RichText::new(if layer == 0 { "Foundation".to_string() } else { format!("Tier {layer}") })
                .small()
                .color(pal.dim),
        );
        // Fixed-width cards in explicit rows (wrapped layouts do not size framed children well).
        let card_w = 330.0;
        let cols = (((ui.available_width() + 8.0) / (card_w + 8.0)) as usize).max(1);
        for row in modules.chunks(cols) {
            ui.horizontal_top(|ui| {
                for m in row {
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| module_card(ui, &content, &game, m, card_w - 26.0, &mut build, &mut study),
                    );
                }
            });
            ui.add_space(4.0);
        }
        ui.add_space(6.0);
    }

    if let Some(id) = build {
        let content = app.content.clone();
        if let Some(g) = app.game.as_mut() {
            match g.start_module_build(&content, &id) {
                Ok(()) => app.toast(ToastKind::Good, "Construction started."),
                Err(e) => app.toast(ToastKind::Warn, e),
            }
        }
    }
    if let Some((challenge, module)) = study {
        app.open_challenge(&challenge, ChallengeContext::Engine { module });
    }
}

fn module_card(
    ui: &mut egui::Ui,
    content: &ContentLibrary,
    game: &GameState,
    m: &EngineModuleDef,
    inner_width: f32,
    build: &mut Option<String>,
    study: &mut Option<(String, String)>,
) {
    let pal = Palette::of(ui);
    let blockers = game.module_blockers(content, m);
    let built = game.engine.has_module(&m.id);
    let building = game.engine.building.as_ref().is_some_and(|b| b.module == m.id);
    let ready = blockers.is_empty();
    let border = if built {
        pal.good
    } else if ready {
        pal.accent
    } else {
        pal.border
    };

    egui::Frame::new()
        .fill(pal.card)
        .stroke(egui::Stroke::new(if ready || built { 1.5 } else { 1.0 }, border))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(inner_width);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&m.name).strong().size(16.0));
                if built {
                    ui.label(RichText::new("✔ built").color(pal.good));
                } else if building {
                    ui.label(RichText::new("building…").color(pal.accent));
                } else if ready {
                    ui.label(RichText::new("ready").color(pal.accent));
                }
            });
            if let Some(t) = content.topic(&m.topic) {
                ui.label(RichText::new(format!("Rust topic: {}", t.name)).small().color(pal.dim));
            }
            ui.label(RichText::new(&m.description).color(pal.dim));

            // What it gives.
            let mut gives: Vec<String> = Category::ALL
                .iter()
                .filter(|c| m.bonus.get(**c) > 0.0)
                .map(|c| format!("{} +{:.0}%", c.label(), m.bonus.get(*c) * 100.0))
                .collect();
            if !m.features.is_empty() {
                gives.push(format!("unlocks: {}", m.features.join(", ")));
            }
            if !gives.is_empty() {
                ui.label(RichText::new(gives.join(" · ")).small().color(pal.info));
            }

            if built {
                return;
            }
            ui.add_space(4.0);
            if !m.requires_modules.is_empty() || !m.required_topic_solves.is_empty() || m.research_cost > 0 {
                for dep in &m.requires_modules {
                    let ok = game.engine.has_module(dep);
                    let name = content.module(dep).map(|d| d.name.as_str()).unwrap_or(dep);
                    req_line(ui, ok, format!("Module: {name}"));
                }
                for (topic, need) in &m.required_topic_solves {
                    let have = game.progress.solved_in_topic(content, topic) as u32;
                    let name = content.topic(topic).map(|t| t.name.as_str()).unwrap_or(topic);
                    req_line(ui, have >= *need, format!("Solve {need} {name} challenges ({have})"));
                }
                req_line(
                    ui,
                    game.studio.research_points >= m.research_cost,
                    format!("{} research points", m.research_cost),
                );
                req_line(ui, game.studio.money >= m.money_cost, fmt::money(m.money_cost));
                if m.min_tier > 0 {
                    req_line(
                        ui,
                        game.studio.tier >= m.min_tier,
                        format!(
                            "Studio: {}",
                            content.tier(m.min_tier).map(|t| t.name.as_str()).unwrap_or("?")
                        ),
                    );
                }
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ready {
                    if widgets::primary_button(ui, &format!("Build ({} wk)", m.build_weeks)).clicked() {
                        *build = Some(m.id.clone());
                    }
                } else if !building {
                    // Show the single most useful next step.
                    if let Some(first) = blockers
                        .iter()
                        .find(|b| !matches!(b, Blocker::OtherBuildInProgress))
                        .or(blockers.first())
                    {
                        ui.label(RichText::new(describe_blocker(content, first)).small().color(pal.warn));
                    }
                }
            });
            let labs = lab_challenges(content, game, m);
            if let Some(first) = labs.first() {
                ui.add_space(2.0);
                if ui
                    .button(format!("Research: {}", first.title))
                    .on_hover_text("Solve this challenge in the Engine lab for full research points.")
                    .clicked()
                {
                    *study = Some((first.id.clone(), m.id.clone()));
                }
            }
        });
}

fn req_line(ui: &mut egui::Ui, ok: bool, text: String) {
    let pal = Palette::of(ui);
    let (mark, color) = if ok { ("✔", pal.good) } else { ("✖", pal.dim) };
    ui.label(RichText::new(format!("{mark} {text}")).small().color(color));
}

//! The market: genre trends, platform life cycles, rival studios and awards.

use eframe::egui::{self, RichText};
use studio_core::sim::GameDate;

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game = game.clone();
    let week = game.date.week();

    ui.heading("Market");
    widgets::dim(
        ui,
        "What players want right now, which platforms are alive, and what your rivals are shipping.",
    );
    ui.add_space(6.0);

    // ---------------------------------------------------------------- trends
    widgets::card(ui, |ui| {
        widgets::section(ui, "Genre trends");
        let mut rows: Vec<(&str, f32)> =
            content.genres.iter().map(|g| (g.name.as_str(), game.market.genre_trend(&g.id))).collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
        if let Some(w) = &game.market.wave {
            let name = content.genre(&w.genre).map(|g| g.name.as_str()).unwrap_or(&w.genre);
            let (word, color) = if w.target > 1.0 { ("booming", pal.good) } else { ("slumping", pal.warn) };
            ui.label(
                RichText::new(format!(
                    "{name} is {word} for another {} week(s).",
                    w.until_week.saturating_sub(week)
                ))
                .color(color),
            );
        }
        for (name, trend) in rows {
            ui.horizontal(|ui| {
                widgets::label_fixed(ui, 120.0, name);
                let color = if trend >= 1.1 {
                    pal.good
                } else if trend <= 0.9 {
                    pal.warn
                } else {
                    pal.info
                };
                widgets::bar(
                    ui,
                    (trend / 1.7).clamp(0.0, 1.0),
                    color,
                    Some(&format!("{:+.0}%", (trend - 1.0) * 100.0)),
                    280.0,
                    18.0,
                );
            });
        }
        widgets::dim(ui, "Demand relative to normal. A game released into a hot genre sells more; rival hits take a share of your audience.");
    });

    // ------------------------------------------------------------- platforms
    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Platforms");
        egui::Grid::new("platforms").num_columns(6).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["Platform", "Type", "Status", "Audience now", "Dev kit", "Needs"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for p in &content.platforms {
                ui.label(RichText::new(&p.name).strong());
                ui.label(format!("{:?}", p.kind));
                let stage = p.stage(week);
                let color = match stage {
                    "growing" | "evergreen" => pal.good,
                    "declining" => pal.warn,
                    "discontinued" => pal.bad,
                    _ => pal.info,
                };
                let status = if stage == "upcoming" {
                    format!("launches in {} wk", p.launch_week.saturating_sub(week))
                } else {
                    stage.to_string()
                };
                ui.label(RichText::new(status).color(color));
                ui.label(format!("×{:.1}", p.market_at(week)));
                ui.label(studio_core::fmt::money(p.dev_cost));
                ui.label(
                    RichText::new(if p.requires_features.is_empty() {
                        "—".into()
                    } else {
                        p.requires_features.join(", ")
                    })
                    .small()
                    .color(pal.dim),
                );
                ui.end_row();
            }
        });
    });

    // ----------------------------------------------------------- competitors
    ui.add_space(6.0);
    ui.columns(2, |cols| {
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Rival releases");
            if game.market.competitor_games.is_empty() {
                widgets::dim(ui, "Nobody has shipped anything yet.");
            }
            for g in game.market.competitor_games.iter().rev().take(14) {
                let genre = content.genre(&g.genre).map(|x| x.name.as_str()).unwrap_or(&g.genre);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{:.0}", g.meta))
                            .strong()
                            .color(widgets::meta_color(&pal, g.meta)),
                    );
                    ui.label(RichText::new(&g.name).strong());
                    ui.label(
                        RichText::new(format!("{genre} — {} ({})", g.studio, GameDate(g.week)))
                            .small()
                            .color(pal.dim),
                    );
                });
            }
        });
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Golden Ferris Awards");
            if game.market.awards.is_empty() {
                widgets::dim(ui, "The first ceremony takes place at the end of year 1.");
            }
            for a in game.market.awards.iter().rev().take(12) {
                let color = if a.player { pal.good } else { pal.text };
                ui.label(RichText::new(format!("Year {} · {}", a.year, a.award)).small().color(pal.dim));
                ui.label(
                    RichText::new(format!(
                        "{} — “{}”{}",
                        a.winner,
                        a.game,
                        if a.player { "  ★ you!" } else { "" }
                    ))
                    .color(color),
                );
            }
        });
    });

    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "The competition");
        for c in &content.competitors {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&c.name).strong());
                ui.label(
                    RichText::new(format!("— {}  (known for {})", c.tagline, c.genres.join(", ")))
                        .small()
                        .color(pal.dim),
                );
            });
        }
    });
}

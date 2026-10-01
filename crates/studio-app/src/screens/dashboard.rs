//! The studio overview.

use eframe::egui::{self, RichText};
use studio_core::fmt;
use studio_core::sim::notify::NoteKind;
use studio_core::sim::progress::level_title;

use crate::app::{App, Nav};
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game = game.clone();
    let mut goto: Option<Nav> = None;
    let mut resume: Option<(String, studio_core::sim::ChallengeContext)> = None;

    ui.heading(format!("Welcome, {}", game.studio.founder));
    widgets::dim(ui, "Press ▶ in the top bar to let time pass. Challenges pause the clock.");
    ui.add_space(8.0);

    if game.debt_weeks > 0 {
        let left = content.balance.bankruptcy_weeks.saturating_sub(game.debt_weeks);
        egui::Frame::new()
            .fill(pal.bad.gamma_multiply(0.18))
            .stroke(egui::Stroke::new(1.0, pal.bad))
            .corner_radius(8)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Bankruptcy warning: you have been in debt for {} week(s). {left} left.",
                        game.debt_weeks
                    ))
                    .strong()
                    .color(pal.bad),
                );
                ui.label("Release a game, cancel costly plans, or take out a loan (Studio screen).");
            });
        ui.add_space(8.0);
    }
    if let Some(a) = &game.pending_attempt {
        let title = content.challenge(&a.challenge_id).map(|c| c.title.clone()).unwrap_or_default();
        egui::Frame::new()
            .fill(pal.warn.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, pal.warn))
            .corner_radius(8)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new(format!("Development is blocked: {title}")).strong().color(pal.warn));
                if widgets::primary_button(ui, "Resume the challenge").clicked() {
                    resume = Some((a.challenge_id.clone(), a.context.clone()));
                }
            });
        ui.add_space(8.0);
    }

    ui.columns(3, |cols| {
        // ---- studio
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Studio");
            ui.label(RichText::new(&game.studio.name).strong().size(18.0));
            if let Some(t) = content.tier(game.studio.tier) {
                ui.label(RichText::new(&t.name).color(pal.accent));
                widgets::dim(ui, &t.description);
                ui.add_space(4.0);
                let (rent, salaries) = game.weekly_fixed_costs(&content);
                ui.label(format!("Rent: {} / week", fmt::money(rent)));
                ui.label(format!("Salaries: {} / week", fmt::money(salaries)));
                ui.label(format!("Team: {} of {} slots", game.staff.len(), t.staff_slots));
            }
            ui.label(format!("Fans: {}", fmt::compact(game.studio.reputation)));
        });
        // ---- finance
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Finances");
            let color = if game.studio.money < 0 { pal.bad } else { pal.good };
            ui.label(RichText::new(fmt::money(game.studio.money)).size(26.0).strong().color(color));
            let avg = game.finances.average_net(8);
            ui.label(
                RichText::new(format!("{} / week (8-week average)", fmt::money(avg))).color(if avg >= 0 {
                    pal.good
                } else {
                    pal.warn
                }),
            );
            match game.runway_weeks(&content) {
                Some(w) => ui.label(format!("Cash covers about {w} week(s) of fixed costs.")),
                None => ui.label("No fixed costs."),
            };
            ui.label(format!("Lifetime revenue: {}", fmt::money(game.finances.lifetime_revenue)));
            let series: Vec<f32> = game.finances.weekly.iter().map(|w| w.revenue.max(0) as f32).collect();
            ui.add_space(4.0);
            widgets::sparkline(ui, &series, egui::vec2(ui.available_width(), 40.0), pal.good);
            widgets::dim(ui, "Weekly revenue");
        });
        // ---- project
        widgets::card(&mut cols[2], |ui| {
            widgets::section(ui, "Current project");
            match &game.project {
                Some(p) => {
                    ui.label(RichText::new(&p.name).strong().size(18.0));
                    ui.label(RichText::new(p.phase().label()).color(pal.accent));
                    widgets::bar(
                        ui,
                        p.fraction(),
                        pal.accent,
                        Some(&format!("{:.0}%", p.fraction() * 100.0)),
                        ui.available_width(),
                        18.0,
                    );
                    if p.is_complete() && p.open_blockers() == 0 {
                        ui.label(RichText::new("Ready to release!").color(pal.good).strong());
                    }
                    if ui.button("Open project").clicked() {
                        goto = Some(Nav::Projects);
                    }
                }
                None => {
                    widgets::dim(ui, "Nothing in development.");
                    if widgets::primary_button(ui, "Start a new project").clicked() {
                        goto = Some(Nav::Projects);
                    }
                }
            }
            if let Some(g) = game.games.last() {
                ui.add_space(6.0);
                ui.label(format!("Latest release: {} ({:.0})", g.name, g.metascore));
            }
        });
    });

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "News");
            if game.feed.is_empty() {
                widgets::dim(ui, "Nothing yet.");
            }
            for n in game.feed.iter().rev().take(10) {
                let color = match n.kind {
                    NoteKind::Info => pal.text,
                    NoteKind::Good => pal.good,
                    NoteKind::Warn => pal.warn,
                    NoteKind::Bad => pal.bad,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("wk {}", n.week)).small().color(pal.dim));
                    ui.label(RichText::new(&n.text).color(color));
                });
            }
        });
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Your Rust journey");
            let level = game.progress.level();
            ui.label(RichText::new(format!("Level {level} — {}", level_title(level))).strong().color(pal.accent));
            let (into, needed) = game.progress.level_progress();
            if let Some(n) = needed {
                widgets::bar(ui, into as f32 / n as f32, pal.accent, Some(&format!("{into} / {n} XP")), ui.available_width(), 18.0);
            }
            ui.add_space(4.0);
            for topic in content.topics.iter().take(5) {
                widgets::meter(ui, &topic.name, game.progress.mastery(&content, &topic.id), pal.accent);
            }
            if ui.button("Study in Skills").clicked() {
                goto = Some(Nav::Skills);
            }
            if game.progress.solved.is_empty() {
                widgets::dim(ui, "Tip: solve your first challenge in Skills — or just start a project and let problems come to you.");
            }
        });
    });

    if let Some(n) = goto {
        app.nav = n;
    }
    if let Some((id, ctx)) = resume {
        app.open_challenge(&id, ctx);
    }
}

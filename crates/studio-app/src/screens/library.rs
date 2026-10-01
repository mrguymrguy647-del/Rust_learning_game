//! Library: every game the studio has released, with its sales.

use eframe::egui::{self, RichText};
use studio_core::fmt;
use studio_core::sim::GameDate;

use crate::app::{App, Nav, ToastKind};
use crate::theme::Palette;
use crate::widgets;

#[derive(Clone, Copy)]
enum Act {
    Patch,
    Dlc,
    Sequel,
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let content = app.content.clone();
    let games = game.games.clone();
    let week = game.date.week();
    let game_state = game.clone();
    let mut open_reviews: Option<usize> = None;
    let mut act: Option<(Act, u32)> = None;

    ui.heading("Library");
    widgets::dim(ui, "Everything you have shipped.");
    ui.add_space(6.0);

    if games.is_empty() {
        widgets::card(ui, |ui| {
            ui.label("You have not released a game yet. Start one in Projects!");
        });
        return;
    }

    let total_rev: i64 = games.iter().map(|g| g.revenue_total).sum();
    let total_profit: i64 = games.iter().map(|g| g.profit()).sum();
    ui.horizontal(|ui| {
        widgets::chip(ui, "Games", &games.len().to_string(), pal.info);
        widgets::chip(ui, "Revenue", &fmt::money(total_rev), pal.good);
        widgets::chip(
            ui,
            "Profit",
            &fmt::money(total_profit),
            if total_profit >= 0 { pal.good } else { pal.bad },
        );
    });
    ui.add_space(6.0);

    for (i, g) in games.iter().enumerate().rev() {
        let genre =
            content.genres.iter().find(|x| x.id == g.genre).map(|x| x.name.clone()).unwrap_or_default();
        let theme =
            content.themes.iter().find(|x| x.id == g.theme).map(|x| x.name.clone()).unwrap_or_default();
        let platform =
            content.platforms.iter().find(|x| x.id == g.platform).map(|x| x.name.clone()).unwrap_or_default();
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:.0}", g.metascore))
                        .size(30.0)
                        .strong()
                        .color(widgets::meta_color(&pal, g.metascore)),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(&g.name).strong().size(17.0));
                    ui.label(
                        RichText::new(format!(
                            "{genre} · {theme} · {platform} · {}  —  released {}",
                            g.size.label(),
                            GameDate(g.release_week)
                        ))
                        .color(pal.dim),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if ui.button("Reviews").clicked() {
                        open_reviews = Some(i);
                    }
                    let status = if g.on_sale {
                        RichText::new("on sale").color(pal.good)
                    } else {
                        RichText::new("sales ended").color(pal.dim)
                    };
                    ui.label(status);
                });
            });
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, "Units", &fmt::compact(g.units_total as i64), pal.info);
                widgets::chip(ui, "Revenue", &fmt::money(g.revenue_total), pal.good);
                widgets::chip(
                    ui,
                    "Profit",
                    &fmt::money(g.profit()),
                    if g.profit() >= 0 { pal.good } else { pal.bad },
                );
                widgets::chip(ui, "Age", &format!("{} wk", g.age(week)), pal.dim);
            });
            // ---- post-launch actions
            if g.is_dlc {
                widgets::dim(ui, "Downloadable content for the game above.");
            } else {
                ui.horizontal_wrapped(|ui| {
                    let patch_problem = game_state.patch_problem(&content, g.id);
                    let patch_label =
                        format!("Patch ({})", fmt::money(game_state.patch_cost(&content, g.id)));
                    let r = ui.add_enabled(patch_problem.is_none(), egui::Button::new(patch_label));
                    let r = r.on_hover_text(patch_problem.clone().unwrap_or_else(|| {
                        "Fix bugs, win back players and give sales a little bump.".into()
                    }));
                    if r.clicked() {
                        act = Some((Act::Patch, g.id));
                    }
                    let dlc_problem = game_state.dlc_problem(g.id);
                    let r = ui.add_enabled(
                        dlc_problem.is_none(),
                        egui::Button::new(format!(
                            "Make DLC ({}/{})",
                            g.dlc_count,
                            studio_core::sim::postlaunch::MAX_DLC
                        )),
                    );
                    let r = r.on_hover_text(
                        dlc_problem
                            .clone()
                            .unwrap_or_else(|| "A small expansion that sells to your existing fans.".into()),
                    );
                    if r.clicked() {
                        act = Some((Act::Dlc, g.id));
                    }
                    let can_sequel = game_state.project.is_none();
                    let r = ui.add_enabled(can_sequel, egui::Button::new("Make a sequel"));
                    let r = r.on_hover_text(if can_sequel {
                        "Reuse the setup and carry your fans over. A good original makes the sequel easier."
                    } else {
                        "Finish the current project first."
                    });
                    if r.clicked() {
                        act = Some((Act::Sequel, g.id));
                    }
                });
            }
            let series: Vec<f32> = g.sales.iter().map(|u| *u as f32).collect();
            widgets::sparkline(ui, &series, egui::vec2(ui.available_width().min(520.0), 46.0), pal.accent);
        });
        ui.add_space(4.0);
    }
    if let Some(i) = open_reviews {
        app.review_popup = Some(i);
    }
    if let Some((a, id)) = act {
        match a {
            Act::Patch => {
                if let Some(g) = app.game.as_mut() {
                    match g.start_patch(&content, id) {
                        Ok(()) => app.toast(ToastKind::Good, "The team is working on a patch."),
                        Err(e) => app.toast(ToastKind::Warn, e),
                    }
                }
            }
            Act::Dlc => {
                if let Some(g) = app.game.as_mut() {
                    match g.start_dlc(&content, id) {
                        Ok(()) => {
                            app.toast(ToastKind::Good, "DLC development started.");
                            app.nav = Nav::Projects;
                        }
                        Err(e) => app.toast(ToastKind::Warn, e),
                    }
                }
            }
            Act::Sequel => {
                if let Some(cfg) = app.game.as_ref().and_then(|g| g.sequel_config(id)) {
                    app.project_form = Some(crate::screens::projects::ProjectForm::from_config(&cfg));
                    app.nav = Nav::Projects;
                }
            }
        }
    }
}

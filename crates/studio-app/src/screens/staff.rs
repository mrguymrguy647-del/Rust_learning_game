//! Staff: your team, hiring, training and morale.

use eframe::egui::{self, RichText};
use studio_core::data::ContentLibrary;
use studio_core::fmt;
use studio_core::sim::hiring::{training_cost, training_weeks};
use studio_core::sim::{GameState, Skill, Staff};

use crate::app::{App, ToastKind};
use crate::theme::Palette;
use crate::widgets;

enum Act {
    Hire(u32),
    Fire(u32),
    Train(u32, Skill),
    Bonus(u32),
    Raise(u32),
    Party,
}

fn skill_row(ui: &mut egui::Ui, s: &Staff) {
    let pal = Palette::of(ui);
    ui.horizontal(|ui| {
        for skill in Skill::ALL {
            let v = s.skill(skill);
            ui.vertical(|ui| {
                ui.label(RichText::new(&skill.label()[..3]).small().color(pal.dim));
                widgets::bar(ui, v / 10.0, pal.accent, Some(&format!("{v:.1}")), 46.0, 16.0);
            });
        }
    });
}

fn trait_labels(ui: &mut egui::Ui, content: &ContentLibrary, s: &Staff) {
    let pal = Palette::of(ui);
    for t in &s.traits {
        if let Some(def) = content.trait_def(t) {
            ui.label(RichText::new(format!("• {}", def.name)).small().color(pal.info))
                .on_hover_text(&def.description);
        }
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game: GameState = game.clone();
    let mut act: Option<Act> = None;

    ui.heading("Staff");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        widgets::chip(ui, "Desks", &format!("{}/{}", game.staff.len(), game.staff_slots(&content)), pal.info);
        let (_, salaries) = game.weekly_fixed_costs(&content);
        widgets::chip(ui, "Salaries", &format!("{}/wk", fmt::money(salaries)), pal.warn);
        widgets::chip(ui, "Senior devs", &game.senior_count(&content).to_string(), pal.good);
        widgets::chip(ui, "Free hints", &game.free_hints(&content).to_string(), pal.accent);
        widgets::chip(
            ui,
            "Contractor discount",
            &fmt::percent(game.contractor_discount(&content)),
            pal.accent,
        );
    });
    widgets::dim(ui, "Developers with programming 7+ (and Rustaceans/Mentors) are your seniors: each covers one hint tier for free and negotiates cheaper contractors.");
    ui.add_space(8.0);

    // ------------------------------------------------------------------ team
    widgets::section(ui, "Your team");
    for s in &game.staff {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&s.name).strong().size(16.0));
                if s.is_founder {
                    ui.label(RichText::new("founder").color(pal.accent));
                } else {
                    ui.label(RichText::new(format!("{}/wk", fmt::money(s.salary))).color(pal.dim));
                }
                if s.is_senior() {
                    ui.label(RichText::new("senior").color(pal.good));
                }
                trait_labels(ui, &content, s);
                if let Some(t) = &s.training {
                    ui.label(
                        RichText::new(format!("training {} ({} wk)", t.skill.label(), t.weeks_left))
                            .color(pal.warn),
                    );
                }
            });
            skill_row(ui, s);
            widgets::meter(ui, "Morale", s.morale / 100.0, widgets::meta_color(&pal, s.morale));
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("Train…", |ui| {
                    for skill in Skill::ALL {
                        let cost = training_cost(s, skill);
                        let weeks = training_weeks(&content, s, skill);
                        let label = format!(
                            "{} ({:.1} → {:.1}) — {}, {weeks} wk",
                            skill.label(),
                            s.skill(skill),
                            (s.skill(skill) + 1.0).min(10.0),
                            fmt::money(cost)
                        );
                        if ui
                            .add_enabled(
                                s.training.is_none() && game.studio.money >= cost,
                                egui::Button::new(label),
                            )
                            .clicked()
                        {
                            act = Some(Act::Train(s.id, skill));
                            ui.close();
                        }
                    }
                });
                if ui
                    .button(format!("Bonus ({})", fmt::money((s.salary * 2).max(200))))
                    .on_hover_text("Morale +20")
                    .clicked()
                {
                    act = Some(Act::Bonus(s.id));
                }
                if !s.is_founder {
                    if ui
                        .button("Raise +10%")
                        .on_hover_text("Permanent salary increase, morale +12")
                        .clicked()
                    {
                        act = Some(Act::Raise(s.id));
                    }
                    if ui
                        .button(RichText::new("Let go").color(pal.bad))
                        .on_hover_text(format!("Severance: {}", fmt::money(s.salary * 4)))
                        .clicked()
                    {
                        act = Some(Act::Fire(s.id));
                    }
                }
            });
        });
        ui.add_space(3.0);
    }
    if ui
        .button(format!("Team party ({})", fmt::money(game.party_cost())))
        .on_hover_text("Morale +10 for everyone")
        .clicked()
    {
        act = Some(Act::Party);
    }

    // ------------------------------------------------------------ candidates
    ui.add_space(10.0);
    widgets::section(ui, "Looking for work");
    widgets::dim(
        ui,
        format!(
            "New candidates arrive every {} weeks. Better studios attract better developers.",
            content.balance.candidate_refresh_weeks
        ),
    );
    let free_desks = game.staff.len() < game.staff_slots(&content);
    for c in &game.candidates {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.name).strong());
                ui.label(RichText::new(format!("{}/wk", fmt::money(c.salary))).color(pal.dim));
                trait_labels(ui, &content, c);
                if c.is_senior() {
                    ui.label(RichText::new("senior").color(pal.good));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fee = game.hire_fee(&content, c);
                    let ok = free_desks && game.studio.money >= fee;
                    let tip = if !free_desks {
                        "No free desks — upgrade your studio.".to_string()
                    } else {
                        format!("Recruiting fee {}", fmt::money(fee))
                    };
                    if ui
                        .add_enabled(ok, egui::Button::new(format!("Hire ({})", fmt::money(fee))))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        act = Some(Act::Hire(c.id));
                    }
                });
            });
            skill_row(ui, c);
        });
        ui.add_space(3.0);
    }

    if let Some(a) = act {
        let content = app.content.clone();
        let result = match (app.game.as_mut(), a) {
            (Some(g), Act::Hire(id)) => g.hire(&content, id).map(|_| "Welcome aboard!".to_string()),
            (Some(g), Act::Fire(id)) => {
                g.fire(id).map(|s| format!("Let go. Severance paid: {}", fmt::money(s)))
            }
            (Some(g), Act::Train(id, skill)) => {
                g.start_training(&content, id, skill).map(|_| "Training started.".to_string())
            }
            (Some(g), Act::Bonus(id)) => g.give_bonus(id).map(|_| "Bonus paid. Morale up!".to_string()),
            (Some(g), Act::Raise(id)) => g.give_raise(id).map(|_| "Raise granted.".to_string()),
            (Some(g), Act::Party) => g.throw_party().map(|_| "Cheers!".to_string()),
            (None, _) => return,
        };
        match result {
            Ok(msg) => app.toast(ToastKind::Good, msg),
            Err(e) => app.toast(ToastKind::Warn, e),
        }
    }
}

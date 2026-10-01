//! Skills: player level, per-topic mastery, achievements — and the entry point for self-study.

use eframe::egui::{self, RichText};
use studio_core::data::Challenge;
use studio_core::sim::progress::{level_title, TOPIC_UNLOCK_THRESHOLD};
use studio_core::sim::{Availability, ChallengeContext};

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

fn stars(d: u8) -> String {
    let d = d.clamp(1, 5) as usize;
    format!("{}{}", "★".repeat(d), "☆".repeat(5 - d))
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let content = app.content.clone();
    let progress = game.progress.clone();
    let mut start: Option<(String, ChallengeContext)> = None;

    ui.add_space(2.0);
    ui.heading("Skills");
    widgets::dim(ui, "Solve challenges to learn Rust. Mastery of a topic opens the next one.");
    ui.add_space(6.0);

    widgets::card(ui, |ui| {
        let level = progress.level();
        let (into, needed) = progress.level_progress();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Level {level}")).size(22.0).strong().color(pal.accent));
            ui.label(RichText::new(level_title(level)).size(18.0));
        });
        match needed {
            Some(n) => {
                let text = format!("{into} / {n} XP to level {}", level + 1);
                widgets::bar(
                    ui,
                    into as f32 / n as f32,
                    pal.accent,
                    Some(&text),
                    ui.available_width().min(520.0),
                    20.0,
                );
            }
            None => {
                ui.label("Maximum level reached. You are a Rustacean Sage.");
            }
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            widgets::chip(
                ui,
                "Solved",
                &format!("{}/{}", progress.solved.len(), content.challenges.len()),
                pal.good,
            );
            widgets::chip(ui, "First try", &progress.stat("first_try_solves").to_string(), pal.info);
            widgets::chip(ui, "Hints used", &progress.stat("hints_used").to_string(), pal.warn);
            widgets::chip(ui, "Contractors", &progress.stat("contractors_hired").to_string(), pal.dim);
        });
    });

    ui.add_space(8.0);
    widgets::section(ui, "Curriculum");
    for (index, topic) in content.topics.iter().enumerate() {
        let challenges: Vec<&Challenge> = content.challenges_in_topic(&topic.id).collect();
        let mastery = progress.mastery(&content, &topic.id);
        let unlocked = progress.topic_unlocked(&content, index, TOPIC_UNLOCK_THRESHOLD);
        let solved = progress.solved_in_topic(&content, &topic.id);
        let header = format!(
            "{}. {} — {}   ({}/{})",
            topic.order,
            topic.name,
            topic.feature_name,
            solved,
            challenges.len()
        );
        let title = if unlocked {
            RichText::new(header).strong()
        } else {
            RichText::new(format!("🔒 {header}")).color(pal.dim)
        };
        egui::CollapsingHeader::new(title)
            .id_salt(("topic", &topic.id))
            .default_open(unlocked && mastery < 1.0 && solved == 0 && index == 0)
            .show(ui, |ui| {
                ui.label(RichText::new(&topic.summary).color(pal.dim));
                widgets::meter(ui, "Mastery", mastery, if mastery >= 1.0 { pal.good } else { pal.accent });
                if !unlocked {
                    if let Some(prev) = content.topics.get(index.wrapping_sub(1)) {
                        ui.label(
                            RichText::new(format!(
                                "Reach {:.0}% mastery in “{}” to unlock this topic.",
                                TOPIC_UNLOCK_THRESHOLD * 100.0,
                                prev.name
                            ))
                            .color(pal.warn),
                        );
                    }
                }
                ui.add_space(4.0);
                for c in &challenges {
                    let avail = progress.availability(&content, c);
                    ui.horizontal(|ui| {
                        let (icon, color) = match avail {
                            Availability::Solved => ("✔", pal.good),
                            Availability::Available => ("○", pal.accent),
                            _ => ("🔒", pal.dim),
                        };
                        ui.label(RichText::new(icon).color(color));
                        widgets::label_fixed(ui, 220.0, RichText::new(&c.title).strong());
                        ui.label(RichText::new(c.kind.label()).color(pal.dim));
                        ui.label(RichText::new(stars(c.difficulty)).color(pal.warn));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match avail {
                            Availability::Available => {
                                if widgets::primary_button(ui, "Study").clicked() {
                                    start = Some((c.id.clone(), ChallengeContext::Study));
                                }
                            }
                            Availability::Solved => {
                                if ui.button("Replay").clicked() {
                                    start = Some((c.id.clone(), ChallengeContext::Practice));
                                }
                                if let Some(rec) = progress.solved.get(&c.id) {
                                    let how = if rec.contractor {
                                        "contractor"
                                    } else if rec.first_try {
                                        "first try"
                                    } else {
                                        "solved"
                                    };
                                    ui.label(RichText::new(how).small().color(pal.dim));
                                }
                            }
                            Availability::LockedPrerequisite => {
                                ui.label(
                                    RichText::new("solve the previous challenge first")
                                        .small()
                                        .color(pal.dim),
                                );
                            }
                            Availability::LockedTopic => {}
                        });
                    });
                }
            });
    }

    ui.add_space(10.0);
    widgets::section(ui, "Achievements");
    for a in &content.achievements {
        let done = progress.achievements.contains(&a.id);
        ui.horizontal(|ui| {
            ui.label(RichText::new(if done { "★" } else { "☆" }).color(if done {
                pal.warn
            } else {
                pal.dim
            }));
            ui.label(RichText::new(&a.name).strong().color(if done { pal.text } else { pal.dim }));
            ui.label(RichText::new(&a.description).color(pal.dim));
        });
    }

    if let Some((id, ctx)) = start {
        app.open_challenge(&id, ctx);
    }
}

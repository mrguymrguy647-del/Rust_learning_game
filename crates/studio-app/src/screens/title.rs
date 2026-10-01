//! Title screen, new-game form and load-game list.

use eframe::egui::{self, RichText};
use studio_core::fmt;
use studio_core::save::{self, AUTOSAVE_SLOT};
use studio_core::settings::Difficulty;
use studio_core::sim::GameDate;

use crate::app::{slot_title, App, View};
use crate::theme::Palette;
use crate::widgets;

fn centered_column(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    let margin = ((avail - width) / 2.0).max(0.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(width);
                add(ui);
            });
        });
    });
}

fn logo(ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    ui.add_space(40.0);
    ui.label(RichText::new("RUST STUDIO").size(46.0).strong().color(pal.accent));
    ui.label(RichText::new("TYCOON").size(46.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Build an engine. Ship games. Learn Rust by writing real code.")
            .size(17.0)
            .color(pal.dim),
    );
    ui.add_space(24.0);
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        centered_column(ui, 520.0, |ui| {
            logo(ui);
            let latest = save::latest_slot(&app.paths);
            let button = |ui: &mut egui::Ui, text: &str| {
                ui.add_sized([ui.available_width(), 40.0], egui::Button::new(RichText::new(text).size(17.0)))
            };

            if let Some(slot) = latest {
                if widgets::primary_button(ui, &format!("Continue  ({})", slot_title(&slot))).clicked() {
                    app.load_slot(&slot);
                }
            }
            if button(ui, "New game").clicked() {
                app.view = View::NewGame;
            }
            if button(ui, "Load game").clicked() {
                app.refresh_slots();
                app.view = View::LoadGame;
            }
            if button(ui, "Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            ui.add_space(24.0);
            widgets::dim(
                ui,
                format!(
                    "{} curriculum topics · {} challenges loaded",
                    app.content.topics.len(),
                    app.content.challenges.len()
                ),
            );
            if !app.content.issues.is_empty() {
                let pal = Palette::of(ui);
                ui.label(
                    RichText::new(format!("{} content file(s) failed to load:", app.content.issues.len()))
                        .color(pal.warn),
                );
                for issue in &app.content.issues {
                    ui.label(RichText::new(issue.to_string()).small().color(pal.warn));
                }
            }
        });
    });
}

pub fn show_new_game(app: &mut App, ui: &mut egui::Ui) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        centered_column(ui, 560.0, |ui| {
            ui.add_space(30.0);
            ui.heading("Found your studio");
            ui.add_space(10.0);
            widgets::card(ui, |ui| {
                ui.label("Studio name");
                ui.add(
                    egui::TextEdit::singleline(&mut app.new_game.studio)
                        .hint_text("e.g. Ferris Games")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                ui.label("Your name");
                ui.add(
                    egui::TextEdit::singleline(&mut app.new_game.founder)
                        .hint_text("Founder")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                ui.label("Economy difficulty");
                ui.horizontal(|ui| {
                    for d in Difficulty::ALL {
                        ui.selectable_value(&mut app.new_game.difficulty, d, d.label());
                    }
                });
                widgets::dim(ui, app.new_game.difficulty.description());
                widgets::dim(ui, "Difficulty only changes money. The Rust challenges are always the same.");
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, "Start in the bedroom").clicked() {
                    app.start_new_game();
                }
                if ui.button("Back").clicked() {
                    app.view = View::Title;
                }
            });
        });
    });
}

pub fn show_load(app: &mut App, ui: &mut egui::Ui) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        centered_column(ui, 760.0, |ui| {
            ui.add_space(30.0);
            ui.heading("Load game");
            ui.add_space(10.0);
            let slots = app.slots.clone();
            let mut to_load: Option<String> = None;
            for info in &slots {
                widgets::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [90.0, 24.0],
                            egui::Label::new(RichText::new(slot_title(&info.slot)).strong()),
                        );
                        match (&info.meta, &info.error) {
                            (Some(m), _) => {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&m.studio_name).strong());
                                    widgets::dim(
                                        ui,
                                        format!("{} · {}", GameDate(m.week), fmt::money(m.money)),
                                    );
                                });
                            }
                            (None, Some(err)) => {
                                let pal = Palette::of(ui);
                                ui.label(RichText::new(format!("Unreadable: {err}")).color(pal.bad));
                            }
                            (None, None) => widgets::dim(ui, "empty"),
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !info.is_empty() {
                                if app.confirm_delete.as_deref() == Some(&info.slot) {
                                    if ui.button("Really delete").clicked() {
                                        if let Err(err) = save::delete_save(&app.paths, &info.slot) {
                                            app.toast(
                                                crate::app::ToastKind::Bad,
                                                format!("Delete failed: {err}"),
                                            );
                                        }
                                        app.confirm_delete = None;
                                        app.refresh_slots();
                                    }
                                } else if ui.button("Delete").clicked() {
                                    app.confirm_delete = Some(info.slot.clone());
                                }
                            }
                            if info.meta.is_some() && widgets::primary_button(ui, "Load").clicked() {
                                to_load = Some(info.slot.clone());
                            }
                        });
                    });
                });
            }
            if let Some(slot) = to_load {
                app.load_slot(&slot);
            }
            ui.add_space(8.0);
            if ui.button("Back").clicked() {
                app.confirm_delete = None;
                app.view = View::Title;
            }
            let _ = AUTOSAVE_SLOT;
        });
    });
}

//! Studio: your office, moving up, loans and the weekly books.

use eframe::egui::{self, RichText};
use studio_core::fmt;

use crate::app::{App, ToastKind};
use crate::theme::Palette;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game = game.clone();
    let mut upgrade = false;
    let mut take: Option<i64> = None;
    let mut repay: Option<i64> = None;

    ui.heading("Studio");
    ui.add_space(4.0);

    ui.columns(2, |cols| {
        // ------------------------------------------------------------ current office
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Your office");
            if let Some(t) = content.tier(game.studio.tier) {
                ui.label(RichText::new(&t.name).strong().size(20.0).color(pal.accent));
                widgets::dim(ui, &t.description);
                ui.add_space(4.0);
                ui.label(format!("Rent: {} / week", fmt::money(t.rent)));
                ui.label(format!("Desks: {}", t.staff_slots));
                ui.label(format!("Hardest blocking challenge: difficulty {}", t.max_difficulty));
                ui.label(format!("Credit limit: {}", fmt::money(t.loan_limit)));
            }
            ui.label(format!("Fans: {}", fmt::compact(game.studio.reputation)));
        });
        // ------------------------------------------------------------ upgrade
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Move to a bigger office");
            match game.upgrade_status(&content) {
                None => {
                    ui.label("You run the biggest studio in the business. Impressive.");
                }
                Some(u) => {
                    ui.label(RichText::new(&u.name).strong().size(18.0));
                    if let Some(t) = content.tier(u.next_tier) {
                        widgets::dim(ui, &t.description);
                        ui.label(format!(
                            "Rent {} / week · {} desks · difficulty {} challenges · credit {}",
                            fmt::money(t.rent),
                            t.staff_slots,
                            t.max_difficulty,
                            fmt::money(t.loan_limit)
                        ));
                    }
                    widgets::meter(
                        ui,
                        "Fans",
                        (u.have_reputation as f32 / u.required_reputation.max(1) as f32).min(1.0),
                        pal.info,
                    );
                    ui.label(format!("Cost: {}", fmt::money(u.cost)));
                    if u.problems.is_empty() {
                        if widgets::primary_button(ui, &format!("Move in ({})", fmt::money(u.cost))).clicked()
                        {
                            upgrade = true;
                        }
                    } else {
                        for p in &u.problems {
                            ui.label(RichText::new(format!("✖ {p}")).small().color(pal.warn));
                        }
                    }
                }
            }
        });
    });

    // ---------------------------------------------------------------- loans
    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Bank");
        let outstanding = game.loan_outstanding();
        let available = game.loan_available(&content);
        ui.horizontal_wrapped(|ui| {
            widgets::chip(
                ui,
                "Outstanding",
                &fmt::money(outstanding),
                if outstanding > 0 { pal.warn } else { pal.good },
            );
            widgets::chip(ui, "Available", &fmt::money(available), pal.info);
            widgets::chip(
                ui,
                "Interest",
                &format!(
                    "{}/wk ({:.1}%)",
                    fmt::money(game.weekly_interest(&content)),
                    content.balance.loan_weekly_rate * 100.0
                ),
                pal.dim,
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let amounts = [5_000i64, 25_000, 100_000];
            for a in amounts {
                if ui
                    .add_enabled(available >= a, egui::Button::new(format!("Borrow {}", fmt::money(a))))
                    .clicked()
                {
                    take = Some(a);
                }
            }
            if ui.add_enabled(available > 0, egui::Button::new("Borrow max")).clicked() {
                take = Some(available);
            }
        });
        ui.horizontal(|ui| {
            for a in [5_000i64, 25_000] {
                if ui
                    .add_enabled(outstanding > 0, egui::Button::new(format!("Repay {}", fmt::money(a))))
                    .clicked()
                {
                    repay = Some(a);
                }
            }
            if ui.add_enabled(outstanding > 0, egui::Button::new("Repay all")).clicked() {
                repay = Some(i64::MAX);
            }
        });
        widgets::dim(ui, "Loans bridge a gap, but interest is charged every week and a negative balance for too long means bankruptcy.");
    });

    // ------------------------------------------------------------- licensing
    ui.add_space(6.0);
    let mut accept: Option<usize> = None;
    let mut decline: Option<usize> = None;
    widgets::card(ui, |ui| {
        widgets::section(ui, "Engine licensing");
        let slots = game.license_slots(&content);
        ui.horizontal_wrapped(|ui| {
            widgets::chip(ui, "Engine value", &game.engine_value(&content).to_string(), pal.accent);
            widgets::chip(ui, "Licensees", &format!("{}/{}", game.licenses.len(), slots), pal.info);
            widgets::chip(ui, "Income", &format!("{}/wk", fmt::money(game.license_income())), pal.good);
        });
        if slots == 0 {
            widgets::dim(ui, "A garage studio is too small to license an engine. Move to a bigger office.");
        } else if game.engine_value(&content) < 150 {
            widgets::dim(
                ui,
                "Build more engine modules: studios only pay for an engine that is worth using.",
            );
        }
        for l in &game.licenses {
            ui.label(format!(
                "{} — {}/week, {} weeks left",
                l.licensee,
                fmt::money(l.weekly_fee),
                l.weeks_left
            ));
        }
        for (i, o) in game.license_offers.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&o.licensee).strong());
                ui.label(format!(
                    "offers {}/week for {} weeks (signing bonus {})",
                    fmt::money(o.weekly_fee),
                    o.term_weeks,
                    fmt::money(o.signing_fee)
                ));
                let free = game.licenses.len() < slots;
                if ui.add_enabled(free, egui::Button::new("Accept")).clicked() {
                    accept = Some(i);
                }
                if ui.button("Decline").clicked() {
                    decline = Some(i);
                }
            });
        }
    });

    // ---------------------------------------------------------------- books
    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "The books (last 12 weeks)");
        egui::Grid::new("books").striped(true).num_columns(7).show(ui, |ui| {
            for h in ["Week", "Revenue", "Rent", "Salaries", "Development", "Other", "Net"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for w in game.finances.weekly.iter().rev().take(12) {
                ui.label(w.week.to_string());
                ui.label(RichText::new(fmt::money(w.revenue)).color(pal.good));
                ui.label(fmt::money(w.rent));
                ui.label(fmt::money(w.salaries));
                ui.label(fmt::money(w.dev));
                ui.label(fmt::money(w.other));
                ui.label(RichText::new(fmt::money(w.net())).color(if w.net() >= 0 {
                    pal.good
                } else {
                    pal.bad
                }));
                ui.end_row();
            }
        });
    });

    if let Some(i) = accept {
        if let Some(g) = app.game.as_mut() {
            if let Err(e) = g.accept_license(&content, i) {
                app.toast(ToastKind::Warn, e);
            }
        }
    }
    if let Some(i) = decline {
        if let Some(g) = app.game.as_mut() {
            g.decline_license(i);
        }
    }
    if upgrade {
        if let Some(g) = app.game.as_mut() {
            match g.upgrade_studio(&content) {
                Ok(()) => app.toast(ToastKind::Good, "Welcome to your new office!"),
                Err(e) => app.toast(ToastKind::Warn, e),
            }
        }
    }
    if let Some(a) = take {
        if let Some(g) = app.game.as_mut() {
            if let Err(e) = g.take_loan(&content, a) {
                app.toast(ToastKind::Warn, e);
            }
        }
    }
    if let Some(a) = repay {
        if let Some(g) = app.game.as_mut() {
            if let Err(e) = g.repay_loan(a) {
                app.toast(ToastKind::Warn, e);
            }
        }
    }
}

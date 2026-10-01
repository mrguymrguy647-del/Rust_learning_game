//! Projects: start a new game (genre, theme, platform, audience, size, focus) and manage the
//! one in development (phases, crunch, blockers, release).

use eframe::egui::{self, RichText};
use studio_core::data::ContentLibrary;
use studio_core::fmt;
use studio_core::sim::quality::focus_fit;
use studio_core::sim::{Audience, Category, GameState, ProjectConfig, ProjectSize, Weights};

use crate::app::App;
use crate::theme::Palette;
use crate::widgets;

/// The new-project form (kept in the app so it survives switching screens).
#[derive(Clone)]
pub struct ProjectForm {
    pub name: String,
    pub genre: String,
    pub theme: String,
    pub platform: String,
    pub audience: Audience,
    pub size: ProjectSize,
    pub focus: Weights,
    /// Follow the genre's ideal split until the player moves a slider.
    pub focus_customized: bool,
}

impl ProjectForm {
    pub fn new(content: &ContentLibrary) -> ProjectForm {
        let genre = content.genres.first();
        ProjectForm {
            name: String::new(),
            genre: genre.map(|g| g.id.clone()).unwrap_or_default(),
            theme: content.themes.first().map(|t| t.id.clone()).unwrap_or_default(),
            platform: content.platforms.first().map(|p| p.id.clone()).unwrap_or_default(),
            audience: Audience::Everyone,
            size: ProjectSize::Small,
            focus: genre.map(|g| g.ideal).unwrap_or(Weights::EVEN),
            focus_customized: false,
        }
    }

    fn config(&self) -> ProjectConfig {
        ProjectConfig {
            name: self.name.clone(),
            genre: self.genre.clone(),
            theme: self.theme.clone(),
            platform: self.platform.clone(),
            audience: self.audience,
            size: self.size,
            focus: self.focus,
            sequel_of: None,
        }
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(game) = app.game.as_ref() else {
        return;
    };
    if game.project.is_some() {
        show_active(app, ui);
    } else {
        show_form(app, ui);
    }
}

// ------------------------------------------------------------------------------------------

fn show_form(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let game: GameState = game.clone();
    let mut form = app.project_form.take().unwrap_or_else(|| ProjectForm::new(&content));
    let mut start = false;

    ui.heading("New project");
    widgets::dim(ui, "Pick what to build. Genre, theme and focus decide how well it fits the market.");
    ui.add_space(6.0);

    // ---- genre
    widgets::card(ui, |ui| {
        widgets::section(ui, "Genre");
        ui.horizontal_wrapped(|ui| {
            for g in &content.genres {
                let missing = game.engine.missing_feature(&content, &g.requires_features);
                let selected = form.genre == g.id;
                let button = egui::Button::new(&g.name).selected(selected);
                let response = ui.add_enabled(missing.is_none(), button);
                let response = response.on_hover_text(match missing {
                    Some(f) => format!("{}\nNeeds the `{f}` engine feature.", g.description),
                    None => g.description.clone(),
                });
                if response.clicked() && !selected {
                    form.genre = g.id.clone();
                    if !form.focus_customized {
                        form.focus = g.ideal;
                    }
                }
            }
        });
        if let Some(g) = content.genres.iter().find(|g| g.id == form.genre) {
            widgets::dim(ui, &g.description);
        }
    });

    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Theme");
        let genre = content.genres.iter().find(|g| g.id == form.genre);
        ui.horizontal_wrapped(|ui| {
            for t in &content.themes {
                let fit = genre.map(|g| g.theme_factor(&t.id)).unwrap_or(1.0);
                let label = if fit >= 1.1 {
                    format!("{} ▲", t.name)
                } else if fit <= 0.9 {
                    format!("{} ▼", t.name)
                } else {
                    t.name.clone()
                };
                let response = ui.add(egui::Button::new(label).selected(form.theme == t.id)).on_hover_text(
                    format!("{}\nFit with this genre: {:+.0}%", t.flavor, (fit - 1.0) * 100.0),
                );
                if response.clicked() {
                    form.theme = t.id.clone();
                }
            }
        });
        widgets::dim(ui, "▲ works great with this genre, ▼ is a risky combination.");
    });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Platform");
            for p in &content.platforms {
                let available = p.available(game.date.week()) && game.studio.tier >= p.min_tier;
                let stage = p.stage(game.date.week());
                let text = format!(
                    "{} — {} (market ×{:.1}, store cut {:.0}%)",
                    p.name,
                    stage,
                    p.market_at(game.date.week()),
                    p.store_cut * 100.0
                );
                if ui
                    .add_enabled(available, egui::Button::new(text).selected(form.platform == p.id))
                    .clicked()
                {
                    form.platform = p.id.clone();
                }
            }
        });
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Audience");
            ui.horizontal(|ui| {
                for a in Audience::ALL {
                    ui.selectable_value(&mut form.audience, a, a.label());
                }
            });
            if let Some(g) = content.genres.iter().find(|g| g.id == form.genre) {
                let fit = g.audience_factor(form.audience.index());
                widgets::dim(ui, format!("Audience fit for {}: {:+.0}% demand", g.name, (fit - 1.0) * 100.0));
            }
        });
    });

    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Size");
        ui.horizontal_wrapped(|ui| {
            for s in ProjectSize::ALL {
                let def = content.balance.size(s);
                let locked = game.studio.tier < def.min_tier;
                let tier_name = content.tier(def.min_tier).map(|t| t.name.clone()).unwrap_or_default();
                let requirement = if locked {
                    format!("needs {tier_name}")
                } else {
                    format!("budget {}", fmt::money(def.budget))
                };
                let label = format!("{}  ·  ${:.2} · {requirement}", s.label(), def.price);
                ui.add_enabled_ui(!locked, |ui| {
                    ui.selectable_value(&mut form.size, s, label);
                });
            }
        });
        let def = content.balance.size(form.size);
        let team = game.team_week(&content, false);
        let weeks = if team.output > 0.0 { (def.work / team.output).ceil() as u32 } else { 0 };
        widgets::dim(ui, format!("Your team makes about {:.0} work per week: roughly {weeks} weeks for this size. {} challenge(s) will block development.", team.output, def.blockers));
    });

    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Focus");
        let ideal = content
            .genres
            .iter()
            .find(|g| g.id == form.genre)
            .map(|g| g.ideal.normalized_to(100.0))
            .unwrap_or(Weights::EVEN);
        for c in Category::ALL {
            ui.horizontal(|ui| {
                widgets::label_fixed(ui, 110.0, c.label());
                let mut v = form.focus.get(c);
                let before = v;
                ui.add(egui::Slider::new(&mut v, 0.0..=100.0).fixed_decimals(0).suffix("%"));
                if (v - before).abs() > 0.4 {
                    form.focus = form.focus.with_adjusted(c, v, 100.0);
                    form.focus_customized = true;
                }
                ui.label(RichText::new(format!("genre ideal {:.0}%", ideal.get(c))).small().color(pal.dim));
            });
        }
        let fit = focus_fit(&form.focus, &ideal);
        ui.horizontal(|ui| {
            ui.label("Genre fit");
            widgets::bar(
                ui,
                fit,
                widgets::meta_color(&pal, fit * 100.0),
                Some(&fmt::percent(fit)),
                220.0,
                18.0,
            );
            if ui.button("Match genre ideal").clicked() {
                form.focus = ideal;
                form.focus_customized = false;
            }
        });
    });

    ui.add_space(6.0);
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut form.name)
                    .hint_text("leave empty for a surprise")
                    .desired_width(260.0),
            );
        });
        let cfg = form.config();
        match game.project_start_problem(&content, &cfg) {
            Some(problem) => {
                ui.label(RichText::new(problem).color(pal.warn));
                ui.add_enabled(false, egui::Button::new("Start development"));
            }
            None => {
                if widgets::primary_button(ui, "Start development").clicked() {
                    start = true;
                }
            }
        }
    });

    if start {
        let cfg = form.config();
        let content = app.content.clone();
        if let Some(game) = app.game.as_mut() {
            match game.start_project(&content, cfg) {
                Ok(()) => {
                    app.toast(crate::app::ToastKind::Good, "Development started. Press ▶ to let time pass.");
                    form = ProjectForm::new(&content);
                }
                Err(msg) => app.toast(crate::app::ToastKind::Warn, msg),
            }
        }
    }
    app.project_form = Some(form);
}

// ------------------------------------------------------------------------------------------

fn show_active(app: &mut App, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let content = app.content.clone();
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let Some(p) = game.project.clone() else {
        return;
    };
    let team = game.team_week(&content, p.crunch);
    let genre = content.genres.iter().find(|g| g.id == p.genre).map(|g| g.name.clone()).unwrap_or_default();
    let theme = content.themes.iter().find(|t| t.id == p.theme).map(|t| t.name.clone()).unwrap_or_default();
    let platform =
        content.platforms.iter().find(|pl| pl.id == p.platform).map(|pl| pl.name.clone()).unwrap_or_default();
    let blocked = game.pending_attempt.clone();
    let bugs = game.project_bug_estimate(&content);
    let remaining = (p.work_total - p.work_done).max(0.0);
    let eta = if team.output > 0.0 { (remaining / team.output).ceil() as u32 } else { 0 };
    let mut release = false;
    let mut resume: Option<(String, studio_core::sim::ChallengeContext)> = None;
    let mut crunch = p.crunch;
    let mut cancel = false;

    ui.heading(&p.name);
    ui.label(
        RichText::new(format!(
            "{genre} · {theme} · {platform} · {} · {}",
            p.audience.label(),
            p.size.label()
        ))
        .color(pal.dim),
    );
    ui.add_space(8.0);

    if let Some(a) = &blocked {
        let title = content.challenge(&a.challenge_id).map(|c| c.title.clone()).unwrap_or_default();
        egui::Frame::new()
            .fill(pal.warn.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, pal.warn))
            .corner_radius(8)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new(format!("Development is blocked: {title}")).strong().color(pal.warn));
                ui.label("The team cannot continue until this problem is solved.");
                if widgets::primary_button(ui, "Resume the challenge").clicked() {
                    resume = Some((a.challenge_id.clone(), a.context.clone()));
                }
            });
        ui.add_space(8.0);
    }

    widgets::card(ui, |ui| {
        widgets::phase_stepper(ui, p.phase(), p.fraction());
        ui.add_space(6.0);
        if p.is_complete() {
            if p.open_blockers() == 0 && blocked.is_none() {
                ui.label(RichText::new("The game is finished!").strong().size(18.0).color(pal.good));
                if widgets::primary_button(ui, "Release the game").clicked() {
                    release = true;
                }
            } else {
                widgets::dim(
                    ui,
                    "Almost there: remaining problems must be solved before release. Let time pass.",
                );
            }
        } else {
            widgets::dim(
                ui,
                format!("About {eta} more week(s) at the current pace ({:.0} work/week).", team.output),
            );
        }
    });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        widgets::card(&mut cols[0], |ui| {
            widgets::section(ui, "Status");
            widgets::meter(
                ui,
                "Estimated bugs",
                bugs / 100.0,
                if bugs > 25.0 {
                    pal.bad
                } else if bugs > 12.0 {
                    pal.warn
                } else {
                    pal.good
                },
            );
            widgets::meter(ui, "Hype", p.hype / 100.0, pal.info);
            widgets::meter(
                ui,
                "Team morale",
                team.avg_morale / 100.0,
                widgets::meta_color(&pal, team.avg_morale),
            );
            ui.label(format!(
                "Spent so far: {}  (budget used {} of {})",
                fmt::money(p.cost_so_far),
                fmt::money(p.budget_spent),
                fmt::money(p.budget_total)
            ));
            ui.label(format!("Quality bonus from solved challenges: +{:.1}", p.challenge_quality));
            ui.add_space(4.0);
            if !p.is_complete() {
                ui.checkbox(&mut crunch, "Crunch time");
                widgets::dim(
                    ui,
                    format!(
                        "Crunch makes the team {:.0}% faster, but costs morale and adds bugs.",
                        (content.balance.crunch_speed - 1.0) * 100.0
                    ),
                );
            }
        });
        widgets::card(&mut cols[1], |ui| {
            widgets::section(ui, "Focus & problems");
            for c in Category::ALL {
                widgets::meter(ui, c.label(), p.focus.get(c) / 100.0, pal.accent);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Blocking problems").strong());
            for (i, b) in p.blockers.iter().enumerate() {
                let (icon, color, text) = if b.resolved {
                    ("✔", pal.good, "solved".to_string())
                } else if b.challenge_id.is_some() {
                    ("!", pal.warn, "blocking now".to_string())
                } else {
                    ("○", pal.dim, format!("expected at {:.0}%", b.at * 100.0))
                };
                ui.label(RichText::new(format!("{icon} Problem {} — {text}", i + 1)).color(color));
            }
        });
    });

    ui.add_space(8.0);
    if app.confirm_cancel_project {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Really cancel? All progress on this game is lost.").color(pal.bad));
            if ui.button("Yes, cancel it").clicked() {
                cancel = true;
            }
            if ui.button("No").clicked() {
                app.confirm_cancel_project = false;
            }
        });
    } else if ui.button("Cancel project…").clicked() {
        app.confirm_cancel_project = true;
    }

    if let Some(game) = app.game.as_mut() {
        if crunch != p.crunch {
            game.set_crunch(crunch);
        }
        if cancel {
            game.cancel_project();
            app.confirm_cancel_project = false;
        }
    }
    if release {
        app.release_project();
    }
    if let Some((id, ctx)) = resume {
        app.open_challenge(&id, ctx);
    }
}

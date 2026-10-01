//! Onboarding: a short tycoon tour for new studios and a quick tour of the code editor the first
//! time a coding challenge opens. Both are skippable and can be replayed from Settings.

use eframe::egui::{self, RichText};
use studio_core::sim::{Availability, ChallengeContext};

use crate::app::{App, Nav};
use crate::theme::Palette;
use crate::widgets;

/// One page of a tour.
pub struct Step {
    /// Screen to show while this page is open.
    pub nav: Option<Nav>,
    pub title: &'static str,
    pub body: &'static str,
    /// Optional call-to-action button on this page.
    pub action: Option<TourAction>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TourAction {
    /// Open the first challenge the player can study.
    OpenFirstChallenge,
}

impl TourAction {
    fn label(self) -> &'static str {
        match self {
            TourAction::OpenFirstChallenge => "Open my first challenge",
        }
    }
}

pub const TYCOON_STEPS: &[Step] = &[
    Step {
        nav: Some(Nav::Dashboard),
        title: "Welcome to Rust Studio Tycoon",
        body: "You run a tiny game studio — and you learn Rust by solving real code problems. This one-minute tour shows how the pieces fit together. You can skip it now and replay it any time from Settings.",
        action: None,
    },
    Step {
        nav: Some(Nav::Dashboard),
        title: "The top bar and time",
        body: "Money, fans (your reputation) and the date live in the top bar. Time only moves while you press ▶ (×1, ×2, ×4); one tick is one week. Whenever a challenge blocks your work, the clock stops until you deal with it.",
        action: None,
    },
    Step {
        nav: Some(Nav::Skills),
        title: "Learn Rust by solving challenges",
        body: "Every challenge is real code, compiled and tested by your own `cargo`. Study any unlocked challenge from the Skills screen for XP, at your own pace, with unlimited retries. Mastering a topic unlocks the next one.",
        action: Some(TourAction::OpenFirstChallenge),
    },
    Step {
        nav: Some(Nav::Engine),
        title: "Build your engine",
        body: "Engine modules improve every game you make and unlock genres, sizes and platforms. Each needs research points, money — and solved challenges in its topic. Your Rust knowledge literally builds the engine.",
        action: None,
    },
    Step {
        nav: Some(Nav::Projects),
        title: "Make a game",
        body: "Choose a genre, theme, platform and size, then set the focus sliders. Development runs through phases; at some points work is blocked until you solve a challenge. A solved challenge adds progress and quality; a failed attempt only costs you time.",
        action: None,
    },
    Step {
        nav: Some(Nav::Staff),
        title: "Your team",
        body: "Hire programmers, designers and artists. Senior developers cover free hints and make contractors cheaper. Watch morale: crunch is fast but risky.",
        action: None,
    },
    Step {
        nav: Some(Nav::Studio),
        title: "Money matters",
        body: "Rent and salaries are paid every week. Loans bridge a gap but charge interest, and staying in the red for too long means bankruptcy. Grow your fans to move into a bigger office.",
        action: None,
    },
    Step {
        nav: Some(Nav::Market),
        title: "Read the market",
        body: "Trends, rival studios and platform lifecycles decide what sells. Check the Market screen before you start a project, and release when the audience is hungry.",
        action: None,
    },
    Step {
        nav: Some(Nav::Codex),
        title: "Codex and Practice",
        body: "Solved challenges unlock Rust Codex entries — a searchable handbook with examples and reading links. Practice lets you replay solved challenges for free, with free hints.",
        action: None,
    },
    Step {
        nav: Some(Nav::Dashboard),
        title: "Your first goals",
        body: "1) Study a challenge or two.  2) Build the Asset manager in Engine.  3) Start a small game in Projects and ship it.  Good luck!",
        action: None,
    },
];

pub const EDITOR_STEPS: &[Step] = &[
    Step {
        nav: None,
        title: "Read the brief",
        body: "The left side tells the story, states the task and lists the signatures your code must provide. Hidden unit tests decide whether you pass; style requirements (if any) are listed below the signatures.",
        action: None,
    },
    Step {
        nav: None,
        title: "The editor",
        body: "Type your Rust in the editor on the right. Tab inserts four spaces, Shift+Tab removes them and Enter keeps the indentation. Lines the compiler complains about are marked.",
        action: None,
    },
    Step {
        nav: None,
        title: "Compile & Test",
        body: "Press “Compile & Test” (or Ctrl+Enter). Your code is written into a throw-away cargo project and checked by the real compiler, then the hidden tests run. The first build can take a little longer; the UI never freezes and you can cancel.",
        action: None,
    },
    Step {
        nav: None,
        title: "Reading the results",
        body: "Compiler errors appear with the error code, line and message — plus a plain-language explanation for common ones. Failed tests show what was expected and what your code returned. Retry as often as you like.",
        action: None,
    },
    Step {
        nav: None,
        title: "Stuck? You are never blocked",
        body: "Hints come in three tiers — a nudge, a direction and partial code — and cost money or a week of research. After a few failed attempts you can hire a contractor: expensive, but you still get the solution and the explanation.",
        action: None,
    },
];

/// Start the tycoon tour (new games, or "replay" in Settings).
pub fn start_tycoon_tour(app: &mut App) {
    app.tutorial = Some(0);
}

/// Start the editor tour (the next time a coding challenge is open).
pub fn start_editor_tour(app: &mut App) {
    app.editor_tour = Some(0);
}

/// Make both tours play again.
pub fn replay(app: &mut App) {
    app.settings.tutorial_done = false;
    app.settings.editor_tutorial_done = false;
    app.mark_settings_dirty();
    start_tycoon_tour(app);
    app.nav = Nav::Dashboard;
}

fn finish_tycoon(app: &mut App) {
    app.tutorial = None;
    if !app.settings.tutorial_done {
        app.settings.tutorial_done = true;
        app.mark_settings_dirty();
    }
    app.nav = Nav::Dashboard;
}

fn finish_editor(app: &mut App) {
    app.editor_tour = None;
    if !app.settings.editor_tutorial_done {
        app.settings.editor_tutorial_done = true;
        app.mark_settings_dirty();
    }
}

enum Pressed {
    Back,
    Next,
    Skip,
    Act(TourAction),
}

/// Draw one tour page; returns what the player pressed.
fn page(ctx: &egui::Context, id: &str, steps: &[Step], index: usize, last_label: &str) -> Option<Pressed> {
    let step = steps.get(index)?;
    let mut pressed: Option<Pressed> = None;
    egui::Window::new(id)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -78.0))
        .fixed_size(egui::vec2(380.0, 0.0))
        .show(ctx, |ui| {
            let pal = Palette::of(ui);
            ui.label(RichText::new(format!("Step {} of {}", index + 1, steps.len())).small().color(pal.dim));
            ui.label(RichText::new(step.title).size(19.0).strong().color(pal.accent));
            ui.add_space(4.0);
            widgets::markup(ui, step.body);
            ui.add_space(8.0);
            if let Some(action) = step.action {
                if widgets::primary_button(ui, action.label()).clicked() {
                    pressed = Some(Pressed::Act(action));
                }
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(index > 0, egui::Button::new("Back")).clicked() {
                    pressed = Some(Pressed::Back);
                }
                let last = index + 1 == steps.len();
                let label = if last { last_label } else { "Next" };
                if ui.add(egui::Button::new(RichText::new(label).strong())).clicked() {
                    pressed = Some(Pressed::Next);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !last && ui.small_button("Skip tour").clicked() {
                        pressed = Some(Pressed::Skip);
                    }
                });
            });
        });
    pressed
}

/// The tycoon tour overlay (drawn above the game screens).
pub fn show_tycoon(app: &mut App, ctx: &egui::Context) {
    let Some(index) = app.tutorial else {
        return;
    };
    // Time stands still while the tour is open.
    app.speed = crate::app::Speed::Paused;
    if index >= TYCOON_STEPS.len() {
        finish_tycoon(app);
        return;
    }
    let Some(pressed) = page(ctx, "tutorial_tycoon", TYCOON_STEPS, index, "Let's go!") else {
        return;
    };
    match pressed {
        Pressed::Skip => tycoon_skip(app),
        Pressed::Back => tycoon_back(app),
        Pressed::Next => tycoon_next(app),
        Pressed::Act(TourAction::OpenFirstChallenge) => open_first_challenge(app),
    }
}

pub fn tycoon_next(app: &mut App) {
    match app.tutorial {
        Some(i) if i + 1 < TYCOON_STEPS.len() => goto_tycoon_step(app, i + 1),
        Some(_) => finish_tycoon(app),
        None => {}
    }
}

pub fn tycoon_back(app: &mut App) {
    if let Some(i) = app.tutorial {
        goto_tycoon_step(app, i.saturating_sub(1));
    }
}

pub fn tycoon_skip(app: &mut App) {
    if app.tutorial.is_some() {
        finish_tycoon(app);
    }
}

fn goto_tycoon_step(app: &mut App, index: usize) {
    app.tutorial = Some(index);
    if let Some(nav) = TYCOON_STEPS.get(index).and_then(|s| s.nav) {
        app.nav = nav;
    }
}

fn open_first_challenge(app: &mut App) {
    let content = app.content.clone();
    let first = app.game.as_ref().and_then(|g| {
        content
            .challenges
            .iter()
            .find(|c| g.progress.availability(&content, c) == Availability::Available)
            .map(|c| c.id.clone())
    });
    if let Some(id) = first {
        // The tour moves on first, so it continues once the player comes back from the challenge.
        tycoon_next(app);
        app.open_challenge(&id, ChallengeContext::Study);
    }
}

/// The editor tour overlay (drawn above the challenge screen).
pub fn show_editor_tour(app: &mut App, ctx: &egui::Context) {
    let Some(index) = app.editor_tour else {
        return;
    };
    if index >= EDITOR_STEPS.len() {
        finish_editor(app);
        return;
    }
    let Some(pressed) = page(ctx, "tutorial_editor", EDITOR_STEPS, index, "Got it") else {
        return;
    };
    match pressed {
        Pressed::Skip => finish_editor(app),
        Pressed::Back => app.editor_tour = Some(index.saturating_sub(1)),
        Pressed::Next => editor_next(app),
        Pressed::Act(_) => {}
    }
}

pub fn editor_next(app: &mut App) {
    match app.editor_tour {
        Some(i) if i + 1 < EDITOR_STEPS.len() => app.editor_tour = Some(i + 1),
        Some(_) => finish_editor(app),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_pages_have_text() {
        for s in TYCOON_STEPS.iter().chain(EDITOR_STEPS) {
            assert!(s.title.len() >= 5, "{}", s.title);
            assert!(s.body.len() >= 60, "{}", s.title);
            assert_eq!(s.body.matches('`').count() % 2, 0, "{}", s.title);
        }
    }

    #[test]
    fn the_tycoon_tour_visits_most_screens() {
        let navs: std::collections::HashSet<Nav> = TYCOON_STEPS.iter().filter_map(|s| s.nav).collect();
        for nav in [Nav::Skills, Nav::Engine, Nav::Projects, Nav::Staff, Nav::Studio, Nav::Market, Nav::Codex]
        {
            assert!(navs.contains(&nav), "{nav:?} is not part of the tour");
        }
    }
}

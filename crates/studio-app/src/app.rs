//! Application state and the top-level screen router.

use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use eframe::egui::{self, RichText};
use studio_core::challenges::{Runner, RunnerConfig, Toolchain, ToolchainError};
use studio_core::data::{ChallengeKind, ContentLibrary};
use studio_core::fmt;
use studio_core::paths::AppPaths;
use studio_core::save::{self, SlotInfo, AUTOSAVE_SLOT};
use studio_core::settings::{Difficulty, Settings};
use studio_core::sim::{Attempt, ChallengeContext, GameState};

use crate::screens::challenge::ChallengeView;
use crate::theme::{self, Palette};
use crate::widgets;

/// Real seconds per in-game week at 1x speed.
const WEEK_SECONDS: f32 = 1.6;
/// Autosave every N in-game weeks.
const AUTOSAVE_EVERY_WEEKS: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Title,
    NewGame,
    LoadGame,
    Game,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Dashboard,
    Projects,
    Engine,
    Staff,
    Studio,
    Market,
    Library,
    Skills,
    Codex,
    Practice,
    Settings,
}

impl Nav {
    pub const ALL: [Nav; 11] = [
        Nav::Dashboard,
        Nav::Projects,
        Nav::Engine,
        Nav::Staff,
        Nav::Studio,
        Nav::Market,
        Nav::Library,
        Nav::Skills,
        Nav::Codex,
        Nav::Practice,
        Nav::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Nav::Dashboard => "Dashboard",
            Nav::Projects => "Projects",
            Nav::Engine => "Engine",
            Nav::Staff => "Staff",
            Nav::Studio => "Studio",
            Nav::Market => "Market",
            Nav::Library => "Library",
            Nav::Skills => "Skills",
            Nav::Codex => "Rust Codex",
            Nav::Practice => "Practice",
            Nav::Settings => "Settings",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Speed {
    Paused,
    X1,
    X2,
    X4,
}

impl Speed {
    fn multiplier(self) -> f32 {
        match self {
            Speed::Paused => 0.0,
            Speed::X1 => 1.0,
            Speed::X2 => 2.0,
            Speed::X4 => 4.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Good,
    Warn,
    Bad,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub age: f32,
}

/// State of the Rust toolchain used to check challenge code.
pub enum ToolchainStatus {
    Detecting,
    Ready(Arc<Runner>),
    Missing(ToolchainError),
}

pub struct NewGameForm {
    pub studio: String,
    pub founder: String,
    pub difficulty: Difficulty,
}

pub struct App {
    pub paths: AppPaths,
    pub settings: Settings,
    pub content: Arc<ContentLibrary>,
    pub view: View,
    pub nav: Nav,
    pub game: Option<GameState>,
    pub speed: Speed,
    pub toasts: Vec<Toast>,
    pub new_game: NewGameForm,
    pub slots: Vec<SlotInfo>,
    pub show_save_dialog: bool,
    pub confirm_delete: Option<String>,
    pub confirm_exit_to_title: bool,
    pub current_slot: Option<String>,
    pub toolchain: ToolchainStatus,
    pub challenge: Option<ChallengeView>,
    pub practice_filter: Option<ChallengeKind>,
    /// The "new project" form (created lazily).
    pub project_form: Option<crate::screens::projects::ProjectForm>,
    /// Index into `game.games` of the release whose reviews are shown.
    pub review_popup: Option<usize>,
    pub confirm_cancel_project: bool,
    /// Egui context, captured on the first frame so background threads can request repaints.
    pub egui_ctx: Option<egui::Context>,
    /// Unsubmitted code of challenges the player walked away from.
    drafts: HashMap<String, String>,
    /// Developer aid (`--dev-screen challenge:<id>:solution`): submit once the toolchain is ready.
    dev_auto_submit: Option<bool>,
    toolchain_rx: Option<mpsc::Receiver<Result<Arc<Runner>, ToolchainError>>>,
    time_acc: f32,
    weeks_since_autosave: u32,
    settings_dirty: bool,
    applied_theme: Option<(bool, u32, u32)>,
}

impl App {
    pub fn new(paths: AppPaths, settings: Settings, content: Arc<ContentLibrary>) -> App {
        let new_game = NewGameForm {
            studio: String::new(),
            founder: String::new(),
            difficulty: settings.default_difficulty,
        };
        let slots = save::list_slots(&paths);
        let mut app = App {
            paths,
            settings,
            content,
            view: View::Title,
            nav: Nav::Dashboard,
            game: None,
            speed: Speed::Paused,
            toasts: Vec::new(),
            new_game,
            slots,
            show_save_dialog: false,
            confirm_delete: None,
            confirm_exit_to_title: false,
            current_slot: None,
            toolchain: ToolchainStatus::Detecting,
            challenge: None,
            practice_filter: None,
            project_form: None,
            review_popup: None,
            confirm_cancel_project: false,
            egui_ctx: None,
            drafts: HashMap::new(),
            dev_auto_submit: None,
            toolchain_rx: None,
            time_acc: 0.0,
            weeks_since_autosave: 0,
            settings_dirty: false,
            applied_theme: None,
        };
        app.redetect_toolchain();
        app
    }

    /// Look for cargo on a background thread; on success pre-build the sandbox so the first
    /// real check is fast.
    pub fn redetect_toolchain(&mut self) {
        self.toolchain = ToolchainStatus::Detecting;
        let (tx, rx) = mpsc::channel();
        self.toolchain_rx = Some(rx);
        let sandbox_root = self.paths.sandbox_dir();
        let config = RunnerConfig::from_settings(&self.settings);
        let ctx = self.egui_ctx.clone();
        std::thread::spawn(move || {
            let result = Toolchain::detect().map(|tc| Arc::new(Runner::new(tc, &sandbox_root, 0, config)));
            let warm = result.as_ref().ok().cloned();
            let _ = tx.send(result);
            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
            if let Some(runner) = warm {
                runner.warm_up();
            }
        });
    }

    fn poll_toolchain(&mut self) {
        let Some(rx) = &self.toolchain_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(runner)) => {
                self.toolchain = ToolchainStatus::Ready(runner);
                self.toolchain_rx = None;
            }
            Ok(Err(err)) => {
                self.toolchain = ToolchainStatus::Missing(err);
                self.toolchain_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.toolchain =
                    ToolchainStatus::Missing(ToolchainError::Broken("detection thread died".into()));
                self.toolchain_rx = None;
            }
        }
    }

    /// Open a challenge. Blocking contexts resume a saved attempt if there is one.
    pub fn open_challenge(&mut self, id: &str, context: ChallengeContext) {
        let Some(challenge) = self.content.challenge(id).cloned() else {
            self.toast(ToastKind::Bad, format!("Unknown challenge `{id}`"));
            return;
        };
        let resumed = self
            .game
            .as_ref()
            .and_then(|g| g.pending_attempt.clone())
            .filter(|a| a.challenge_id == id && a.context == context);
        let mut attempt = resumed.unwrap_or_else(|| Attempt::new(&challenge, context));
        if let Some(draft) = self.drafts.get(id) {
            if attempt.submissions == 0 {
                attempt.code = draft.clone();
            }
        }
        self.challenge = Some(ChallengeView::new(challenge, attempt, self.nav));
        self.speed = Speed::Paused;
    }

    /// Remember what the player typed so leaving and coming back does not lose it.
    pub fn stash_draft(&mut self, id: &str, code: &str, solved: bool) {
        if solved {
            self.drafts.remove(id);
        } else {
            self.drafts.insert(id.to_string(), code.to_string());
        }
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), kind, age: 0.0 });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn mark_settings_dirty(&mut self) {
        self.settings_dirty = true;
    }

    pub fn refresh_slots(&mut self) {
        self.slots = save::list_slots(&self.paths);
    }

    /// Called once per frame by the eframe adapter.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.egui_ctx.is_none() {
            self.egui_ctx = Some(ctx.clone());
        }
        self.poll_toolchain();
        if let (Some(use_solution), ToolchainStatus::Ready(_)) = (self.dev_auto_submit, &self.toolchain) {
            self.dev_auto_submit = None;
            crate::screens::challenge::dev_submit(self, use_solution);
        }
        self.apply_theme_if_changed(&ctx);

        match self.view {
            View::Title => crate::screens::title::show(self, ui),
            View::NewGame => crate::screens::title::show_new_game(self, ui),
            View::LoadGame => crate::screens::title::show_load(self, ui),
            View::Game => {
                self.tick_clock(ui);
                self.show_game(ui);
            }
        }

        self.toasts_ui(&ctx);
        self.persist_settings_if_dirty();
    }

    /// Developer shortcut (`--dev-screen <spec>`): jump straight to a screen for screenshots.
    /// Specs: `new`, `load`, `game`, `game:<Nav label>`.
    pub fn dev_jump(&mut self, spec: &str) {
        let (head, tail) = spec.split_once(':').unwrap_or((spec, ""));
        match head {
            "new" => self.view = View::NewGame,
            "load" => {
                self.refresh_slots();
                self.view = View::LoadGame;
            }
            "game" => {
                self.new_game.studio = "Ferris Games".into();
                self.new_game.founder = "Alex".into();
                self.start_new_game();
                let (nav_name, flag) = tail.split_once(':').unwrap_or((tail, ""));
                if let Some(nav) = Nav::ALL.iter().find(|n| n.label().eq_ignore_ascii_case(nav_name)) {
                    self.nav = *nav;
                }
                if flag == "rich" {
                    self.dev_enrich();
                }
            }
            // `sim:<Nav>:<weeks>[:release]` — play a puzzle project for N weeks with auto-solved blockers.
            "sim" => {
                let mut parts = tail.split(':');
                let nav = parts.next().unwrap_or("Dashboard");
                let weeks: u32 = parts.next().and_then(|w| w.parse().ok()).unwrap_or(10);
                let release = parts.next() == Some("release");
                self.dev_simulate(weeks, release);
                if let Some(n) = Nav::ALL.iter().find(|n| n.label().eq_ignore_ascii_case(nav)) {
                    self.nav = *n;
                }
            }
            "challenge" => {
                self.new_game.studio = "Ferris Games".into();
                self.start_new_game();
                let mut parts = tail.split(':');
                let id = parts.next().unwrap_or_default().to_string();
                self.open_challenge(&id, ChallengeContext::Study);
                match parts.next() {
                    Some("solution") => self.dev_auto_submit = Some(true),
                    Some("starter") => self.dev_auto_submit = Some(false),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Developer aid (`game:<Nav>:rich`): money, fans, research, a bigger office and some progress.
    fn dev_enrich(&mut self) {
        let content = self.content.clone();
        let Some(game) = self.game.as_mut() else {
            return;
        };
        game.studio.money = 250_000;
        game.studio.reputation = 2_000;
        game.studio.research_points = 400;
        game.studio.tier = 2;
        let ids: Vec<String> = content.challenges.iter().take(8).map(|c| c.id.clone()).collect();
        for id in ids {
            game.progress
                .solved
                .insert(id, studio_core::sim::SolveRecord { first_try: true, ..Default::default() });
        }
        game.engine.built.insert("asset_manager".into());
        game.refresh_candidates(&content);
        for _ in 0..2 {
            if let Some(c) = game.candidates.first().map(|c| c.id) {
                let _ = game.hire(&content, c);
            }
        }
        let _ = game.take_loan(&content, 20_000);
        for _ in 0..8 {
            game.advance_week(&content);
        }
    }

    /// Developer aid: fast-forward a fresh game for screenshots (blockers are solved instantly).
    fn dev_simulate(&mut self, weeks: u32, release: bool) {
        use studio_core::sim::{Audience, ProjectConfig, ProjectSize};
        self.new_game.studio = "Ferris Games".into();
        self.new_game.founder = "Alex".into();
        self.start_new_game();
        let content = self.content.clone();
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let Some(genre) = content.genres.first() else {
            return;
        };
        let cfg = ProjectConfig {
            name: String::new(),
            genre: genre.id.clone(),
            theme: "cooking".into(),
            platform: "pc".into(),
            audience: Audience::Everyone,
            size: ProjectSize::Small,
            focus: genre.ideal,
            sequel_of: None,
        };
        let _ = game.start_project(&content, cfg);
        for _ in 0..weeks {
            while let Some(a) = game.pending_attempt.clone() {
                if let Some(c) = content.challenge(&a.challenge_id) {
                    let mut done = a.clone();
                    done.submissions = 1;
                    game.complete_challenge(&content, c, &done);
                }
                game.pending_attempt = None;
            }
            game.advance_week(&content);
        }
        if release && game.release_project(&content).is_ok() {
            for _ in 0..6 {
                game.advance_week(&content);
            }
            self.review_popup = Some(0);
        }
    }

    pub fn on_exit(&mut self) {
        if self.view == View::Game {
            self.autosave();
        }
        self.persist_settings_if_dirty();
    }

    fn apply_theme_if_changed(&mut self, ctx: &egui::Context) {
        let key = (
            self.settings.dark_mode,
            (self.settings.editor_font_size * 10.0) as u32,
            (self.settings.ui_scale * 100.0) as u32,
        );
        if self.applied_theme != Some(key) {
            theme::apply(ctx, &self.settings);
            self.applied_theme = Some(key);
        }
    }

    fn persist_settings_if_dirty(&mut self) {
        if self.settings_dirty {
            self.settings_dirty = false;
            if let Err(err) = self.settings.save(&self.paths) {
                self.toast(ToastKind::Warn, format!("Could not save settings: {err}"));
            }
        }
    }

    // ----- game lifecycle ------------------------------------------------------------

    pub fn start_new_game(&mut self) {
        let form = &self.new_game;
        let state = GameState::new_game(&self.content, &form.studio, &form.founder, form.difficulty, None);
        self.game = Some(state);
        self.project_form = None;
        self.review_popup = None;
        self.current_slot = Some(AUTOSAVE_SLOT.to_string());
        self.view = View::Game;
        self.nav = Nav::Dashboard;
        self.speed = Speed::Paused;
        self.time_acc = 0.0;
        self.weeks_since_autosave = 0;
        self.autosave();
        self.toast(ToastKind::Info, "Studio founded. Good luck!");
    }

    pub fn load_slot(&mut self, slot: &str) {
        match save::load_game(&self.paths, slot) {
            Ok(file) => {
                self.game = Some(file.state);
                self.current_slot = Some(slot.to_string());
                self.view = View::Game;
                self.nav = Nav::Dashboard;
                self.speed = Speed::Paused;
                self.time_acc = 0.0;
                self.weeks_since_autosave = 0;
                self.settings.last_slot = Some(slot.to_string());
                self.mark_settings_dirty();
                self.toast(ToastKind::Good, "Game loaded");
                if let Some(a) = self.game.as_ref().and_then(|g| g.pending_attempt.clone()) {
                    self.open_challenge(&a.challenge_id, a.context);
                }
            }
            Err(err) => self.toast(ToastKind::Bad, format!("Could not load: {err}")),
        }
    }

    pub fn save_to_slot(&mut self, slot: &str) {
        let Some(game) = &self.game else {
            return;
        };
        match save::save_game(&self.paths, slot, game) {
            Ok(()) => {
                self.current_slot = Some(slot.to_string());
                self.settings.last_slot = Some(slot.to_string());
                self.mark_settings_dirty();
                self.refresh_slots();
                self.toast(ToastKind::Good, "Game saved");
            }
            Err(err) => self.toast(ToastKind::Bad, format!("Save failed: {err}")),
        }
    }

    pub fn autosave(&mut self) {
        if !self.settings.autosave {
            return;
        }
        if let Some(game) = &self.game {
            if let Err(err) = save::save_game(&self.paths, AUTOSAVE_SLOT, game) {
                self.toast(ToastKind::Warn, format!("Autosave failed: {err}"));
            }
        }
    }

    pub fn exit_to_title(&mut self) {
        self.autosave();
        self.game = None;
        self.view = View::Title;
        self.speed = Speed::Paused;
        self.refresh_slots();
    }

    // ----- clock -----------------------------------------------------------------------

    fn tick_clock(&mut self, ui: &egui::Ui) {
        if self.speed == Speed::Paused || self.game.is_none() || self.challenge.is_some() {
            return;
        }
        // Pressing play while development is blocked reopens the blocking challenge.
        if let Some(a) = self.game.as_ref().and_then(|g| g.pending_attempt.clone()) {
            self.speed = Speed::Paused;
            self.open_challenge(&a.challenge_id, a.context);
            return;
        }
        if self.game.as_ref().is_some_and(|g| g.game_over.is_some()) {
            self.speed = Speed::Paused;
            return;
        }
        let dt = ui.input(|i| i.stable_dt).min(0.25);
        self.time_acc += dt * self.speed.multiplier();
        while self.time_acc >= WEEK_SECONDS {
            self.time_acc -= WEEK_SECONDS;
            self.advance_one_week();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(80));
    }

    /// One simulated week. A blocking challenge pauses the clock and opens the editor.
    pub fn advance_one_week(&mut self) {
        let content = self.content.clone();
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let feed_len = game.feed.len();
        let was_over = game.game_over.is_some();
        game.advance_week(&content);

        // Surface the newest bad/good news as toasts so the player notices without opening the dashboard.
        let fresh: Vec<_> = game.feed.iter().skip(feed_len.min(game.feed.len())).cloned().collect();
        let blocked = game.pending_attempt.clone();
        let over = game.game_over.is_some() && !was_over;
        for n in fresh {
            let kind = match n.kind {
                studio_core::sim::notify::NoteKind::Info => ToastKind::Info,
                studio_core::sim::notify::NoteKind::Good => ToastKind::Good,
                studio_core::sim::notify::NoteKind::Warn => ToastKind::Warn,
                studio_core::sim::notify::NoteKind::Bad => ToastKind::Bad,
            };
            self.toast(kind, n.text);
        }

        self.weeks_since_autosave += 1;
        if self.weeks_since_autosave >= AUTOSAVE_EVERY_WEEKS || blocked.is_some() || over {
            self.weeks_since_autosave = 0;
            self.autosave();
        }
        if over || blocked.is_some() {
            self.speed = Speed::Paused;
        }
        if let Some(a) = blocked {
            if self.challenge.is_none() {
                self.open_challenge(&a.challenge_id, a.context);
            }
        }
    }

    /// Release the finished project and show its reviews.
    pub fn release_project(&mut self) {
        let content = self.content.clone();
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.release_project(&content) {
            Ok(idx) => {
                self.review_popup = Some(idx);
                self.speed = Speed::Paused;
                self.autosave();
            }
            Err(msg) => self.toast(ToastKind::Warn, msg),
        }
    }

    // ----- in-game shell -----------------------------------------------------------------

    fn show_game(&mut self, ui: &mut egui::Ui) {
        if self.challenge.is_some() {
            crate::screens::challenge::show(self, ui);
            return;
        }
        let pal = Palette::of(ui);

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });

        egui::Panel::left("nav")
            .exact_size(176.0)
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::same(10)))
            .show(ui, |ui| {
                self.nav_bar(ui);
            });

        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.nav {
                Nav::Dashboard => crate::screens::dashboard::show(self, ui),
                Nav::Projects => crate::screens::projects::show(self, ui),
                Nav::Engine => crate::screens::engine::show(self, ui),
                Nav::Staff => crate::screens::staff::show(self, ui),
                Nav::Studio => crate::screens::studio::show(self, ui),
                Nav::Library => crate::screens::library::show(self, ui),
                Nav::Skills => crate::screens::skills::show(self, ui),
                Nav::Practice => crate::screens::practice::show(self, ui),
                Nav::Settings => crate::screens::settings::show(self, ui),
                other => crate::screens::placeholder::show(self, ui, other),
            });
        });

        self.save_dialog(ui.ctx());
        self.exit_dialog(ui.ctx());
        crate::screens::reviews::show_popup(self, ui.ctx());
        crate::screens::gameover::show(self, ui.ctx());
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = Palette::of(ui);
        let Some(game) = &self.game else {
            return;
        };
        let money = game.studio.money;
        let title = game.studio.name.clone();
        let date = game.date.to_string();
        let rep = game.studio.reputation;
        let tier_name =
            self.content.tier(game.studio.tier).map(|t| t.name.clone()).unwrap_or_else(|| "Studio".into());

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(title).strong().size(18.0).color(pal.accent));
            ui.label(RichText::new(tier_name).color(pal.dim));
            ui.separator();
            ui.label(date);
            ui.separator();
            let money_color = if money < 0 { pal.bad } else { pal.good };
            widgets::chip(ui, "Money", &fmt::money(money), money_color);
            widgets::chip(ui, "Fans", &fmt::compact(rep), pal.info);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.button("Menu").clicked() {
                    self.confirm_exit_to_title = true;
                }
                if ui.button("Save").clicked() {
                    self.refresh_slots();
                    self.show_save_dialog = true;
                }
                ui.separator();
                for (label, speed) in
                    [("▶▶▶", Speed::X4), ("▶▶", Speed::X2), ("▶", Speed::X1), ("⏸", Speed::Paused)]
                {
                    let selected = self.speed == speed;
                    if ui.selectable_label(selected, label).clicked() {
                        self.speed = speed;
                    }
                }
                ui.label(RichText::new("Time").small().color(pal.dim));
            });
        });
    }

    fn nav_bar(&mut self, ui: &mut egui::Ui) {
        let pal = Palette::of(ui);
        ui.add_space(4.0);
        for nav in Nav::ALL {
            let selected = self.nav == nav;
            let text = if selected {
                RichText::new(nav.label()).strong().color(pal.accent_text)
            } else {
                RichText::new(nav.label())
            };
            let button = egui::Button::new(text)
                .fill(if selected { pal.accent } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(ui.available_width(), 32.0));
            if ui.add(button).clicked() {
                self.nav = nav;
            }
        }
    }

    fn save_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_save_dialog {
            return;
        }
        let mut open = true;
        let mut chosen: Option<String> = None;
        egui::Window::new("Save game")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Choose a slot. Autosave runs automatically and is not listed here.");
                ui.add_space(6.0);
                for info in self.slots.iter().filter(|s| s.slot != AUTOSAVE_SLOT) {
                    ui.horizontal(|ui| {
                        ui.add_sized([70.0, 24.0], egui::Label::new(slot_title(&info.slot)));
                        let summary = match (&info.meta, &info.error) {
                            (Some(m), _) => format!(
                                "{} — {} — {}",
                                m.studio_name,
                                studio_core::sim::GameDate(m.week),
                                fmt::money(m.money)
                            ),
                            (None, Some(_)) => "unreadable file (will be overwritten)".to_string(),
                            (None, None) => "empty".to_string(),
                        };
                        ui.label(summary);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if info.is_empty() { "Save" } else { "Overwrite" };
                            if ui.button(label).clicked() {
                                chosen = Some(info.slot.clone());
                            }
                        });
                    });
                }
            });
        if let Some(slot) = chosen {
            self.save_to_slot(&slot);
            self.show_save_dialog = false;
        }
        if !open {
            self.show_save_dialog = false;
        }
    }

    fn exit_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_exit_to_title {
            return;
        }
        let mut stay = false;
        let mut leave = false;
        egui::Window::new("Back to main menu?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Your progress is autosaved when you leave.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Save & leave").clicked() {
                        leave = true;
                    }
                    if ui.button("Keep playing").clicked() {
                        stay = true;
                    }
                });
            });
        if leave {
            self.confirm_exit_to_title = false;
            self.exit_to_title();
        } else if stay {
            self.confirm_exit_to_title = false;
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.25);
        for t in &mut self.toasts {
            t.age += dt;
        }
        self.toasts.retain(|t| t.age < 5.0);
        if self.toasts.is_empty() {
            return;
        }
        let pal = Palette::from_dark(self.settings.dark_mode);
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let color = match t.kind {
                        ToastKind::Info => pal.info,
                        ToastKind::Good => pal.good,
                        ToastKind::Warn => pal.warn,
                        ToastKind::Bad => pal.bad,
                    };
                    egui::Frame::new()
                        .fill(pal.card)
                        .stroke(egui::Stroke::new(1.5, color))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&t.text).color(pal.text));
                        });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

pub fn slot_title(slot: &str) -> String {
    match slot {
        AUTOSAVE_SLOT => "Autosave".to_string(),
        other => other.strip_prefix("slot").map(|n| format!("Slot {n}")).unwrap_or_else(|| other.to_string()),
    }
}

/// An app with its own temp data dir, for tests in any module.
#[cfg(test)]
pub fn test_app(name: &str) -> App {
    let dir = std::env::temp_dir().join(format!("rst_app_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = AppPaths::at(dir);
    App::new(paths, Settings::default(), Arc::new(ContentLibrary::embedded()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `frames` headless egui passes.
    fn run_frames(app: &mut App, frames: usize) {
        let ctx = egui::Context::default();
        for _ in 0..frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1360.0, 840.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.ui(ui));
            output.textures_delta.clear();
        }
    }

    #[test]
    fn every_screen_renders_without_panicking() {
        let mut app = test_app("screens");
        run_frames(&mut app, 2);
        app.view = View::NewGame;
        run_frames(&mut app, 2);
        app.view = View::LoadGame;
        run_frames(&mut app, 2);

        app.new_game.studio = "Test Studio".into();
        app.start_new_game();
        assert_eq!(app.view, View::Game);
        for nav in Nav::ALL {
            app.nav = nav;
            run_frames(&mut app, 2);
        }
        app.show_save_dialog = true;
        run_frames(&mut app, 2);
        app.confirm_exit_to_title = true;
        run_frames(&mut app, 2);
        app.confirm_exit_to_title = false;

        // Every challenge (code and quiz) must be displayable.
        let ids: Vec<String> = app.content.challenges.iter().map(|c| c.id.clone()).collect();
        for id in ids {
            app.open_challenge(&id, ChallengeContext::Study);
            run_frames(&mut app, 2);
            app.challenge = None;
        }
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn tycoon_screens_render_in_every_game_state() {
        let mut app = test_app("states");
        // Mid-project.
        app.dev_simulate(12, false);
        for nav in Nav::ALL {
            app.nav = nav;
            run_frames(&mut app, 2);
        }
        // A well-funded studio with staff, loans and built modules.
        app.dev_jump("game:Dashboard:rich");
        for nav in Nav::ALL {
            app.nav = nav;
            run_frames(&mut app, 2);
        }
        // After a release, with the review window open.
        app.dev_simulate(60, true);
        assert!(app.review_popup.is_some());
        for nav in Nav::ALL {
            app.nav = nav;
            run_frames(&mut app, 2);
        }
        app.review_popup = None;
        // Blocked by a challenge (blocker open on screen), then bankrupt.
        app.dev_simulate(0, false);
        if let Some(game) = app.game.as_mut() {
            game.studio.money = -1;
            game.debt_weeks = 3;
        }
        for nav in [Nav::Dashboard, Nav::Projects] {
            app.nav = nav;
            run_frames(&mut app, 2);
        }
        if let Some(game) = app.game.as_mut() {
            game.game_over = Some(studio_core::sim::economy::GameOver { week: 5, reason: "test".into() });
        }
        run_frames(&mut app, 2);
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn a_blocker_opens_the_challenge_automatically_and_pauses_the_clock() {
        use studio_core::sim::{Audience, ProjectConfig, ProjectSize};
        let mut app = test_app("blocker");
        app.new_game.studio = "T".into();
        app.start_new_game();
        let content = app.content.clone();
        let genre = content.genres.first().unwrap();
        let cfg = ProjectConfig {
            name: "X".into(),
            genre: genre.id.clone(),
            theme: "cooking".into(),
            platform: "pc".into(),
            audience: Audience::Everyone,
            size: ProjectSize::Small,
            focus: genre.ideal,
            sequel_of: None,
        };
        app.game.as_mut().unwrap().start_project(&content, cfg).unwrap();
        app.speed = Speed::X4;
        for _ in 0..200 {
            if app.challenge.is_some() {
                break;
            }
            app.advance_one_week();
        }
        let view = app.challenge.as_ref().expect("the blocking challenge opened by itself");
        assert_eq!(view.attempt.context, ChallengeContext::Project);
        assert_eq!(app.speed, Speed::Paused);
        assert!(app.game.as_ref().unwrap().pending_attempt.is_some());
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn new_game_autosaves_and_can_be_loaded_again() {
        let mut app = test_app("lifecycle");
        app.new_game.studio = "Persist Games".into();
        app.start_new_game();
        app.advance_one_week();
        app.save_to_slot("slot2");
        app.exit_to_title();
        assert_eq!(app.view, View::Title);
        app.load_slot("slot2");
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.studio.name, "Persist Games");
        assert_eq!(game.date.week(), 1);
        let _ = std::fs::remove_dir_all(app.paths.root());
    }
}

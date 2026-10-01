//! Application state and the top-level screen router.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, RichText};
use studio_core::data::ContentLibrary;
use studio_core::fmt;
use studio_core::paths::AppPaths;
use studio_core::save::{self, SlotInfo, AUTOSAVE_SLOT};
use studio_core::settings::{Difficulty, Settings};
use studio_core::sim::GameState;

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
        App {
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
            time_acc: 0.0,
            weeks_since_autosave: 0,
            settings_dirty: false,
            applied_theme: None,
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
                if let Some(nav) = Nav::ALL.iter().find(|n| n.label().eq_ignore_ascii_case(tail)) {
                    self.nav = *nav;
                }
            }
            _ => {}
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
        let state = GameState::new_game(&form.studio, &form.founder, form.difficulty, None);
        self.game = Some(state);
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
        if self.speed == Speed::Paused || self.game.is_none() {
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

    pub fn advance_one_week(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        game.date.advance(1);
        self.weeks_since_autosave += 1;
        if self.weeks_since_autosave >= AUTOSAVE_EVERY_WEEKS {
            self.weeks_since_autosave = 0;
            self.autosave();
        }
    }

    // ----- in-game shell -----------------------------------------------------------------

    fn show_game(&mut self, ui: &mut egui::Ui) {
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
                Nav::Settings => crate::screens::settings::show(self, ui),
                other => crate::screens::placeholder::show(self, ui, other),
            });
        });

        self.save_dialog(ui.ctx());
        self.exit_dialog(ui.ctx());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(name: &str) -> App {
        let dir = std::env::temp_dir().join(format!("rst_app_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = AppPaths::at(dir);
        App::new(paths, Settings::default(), Arc::new(ContentLibrary::embedded()))
    }

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

//! User settings (persisted separately from save games).

use std::fs;
use std::io;

use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;

/// Economy difficulty. Affects money only — never the learning content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    }

    /// Multiplier on revenue.
    pub fn revenue_mult(self) -> f32 {
        match self {
            Difficulty::Easy => 1.25,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 0.8,
        }
    }

    /// Multiplier on salaries, rent and other running costs.
    pub fn cost_mult(self) -> f32 {
        match self {
            Difficulty::Easy => 0.8,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 1.25,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Difficulty::Easy => "More revenue, lower costs. Relaxed.",
            Difficulty::Normal => "Balanced.",
            Difficulty::Hard => "Less revenue, higher costs. Money is tight.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Difficulty used for new games (and shown in the settings screen).
    pub default_difficulty: Difficulty,
    /// Scales the in-game money cost of hints (0 = free).
    pub hint_cost_multiplier: f32,
    /// Seconds the compiled tests may run before being killed.
    pub sandbox_timeout_secs: u64,
    /// Seconds `cargo` may spend compiling before being killed.
    pub compile_timeout_secs: u64,
    pub editor_font_size: f32,
    pub ui_scale: f32,
    pub dark_mode: bool,
    pub autosave: bool,
    /// Failed attempts before a contractor can be hired.
    pub contractor_after_failures: u32,
    pub tutorial_done: bool,
    pub editor_tutorial_done: bool,
    /// Last used save slot (for "Continue").
    pub last_slot: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_difficulty: Difficulty::Normal,
            hint_cost_multiplier: 1.0,
            sandbox_timeout_secs: 20,
            compile_timeout_secs: 120,
            editor_font_size: 15.0,
            ui_scale: 1.0,
            dark_mode: true,
            autosave: true,
            contractor_after_failures: 3,
            tutorial_done: false,
            editor_tutorial_done: false,
            last_slot: None,
        }
    }
}

impl Settings {
    /// Keep every value inside a sane range (hand-edited files, old versions…).
    pub fn sanitized(mut self) -> Settings {
        self.hint_cost_multiplier = self.hint_cost_multiplier.clamp(0.0, 5.0);
        self.sandbox_timeout_secs = self.sandbox_timeout_secs.clamp(3, 300);
        self.compile_timeout_secs = self.compile_timeout_secs.clamp(20, 900);
        self.editor_font_size = self.editor_font_size.clamp(8.0, 40.0);
        self.ui_scale = self.ui_scale.clamp(0.75, 2.5);
        self.contractor_after_failures = self.contractor_after_failures.clamp(1, 20);
        self
    }

    /// Loads settings; a missing or corrupt file yields defaults (never an error).
    pub fn load(paths: &AppPaths) -> Settings {
        fs::read_to_string(paths.settings_file())
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self, paths: &AppPaths) -> io::Result<()> {
        fs::create_dir_all(paths.root())?;
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        crate::save::atomic_write(&paths.settings_file(), text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> AppPaths {
        let dir = std::env::temp_dir().join(format!("rst_settings_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        AppPaths::at(dir)
    }

    #[test]
    fn round_trip_and_defaults() {
        let paths = temp_paths("rt");
        assert_eq!(Settings::load(&paths), Settings::default());
        let s = Settings { editor_font_size: 20.0, hint_cost_multiplier: 0.5, ..Settings::default() };
        s.save(&paths).unwrap();
        assert_eq!(Settings::load(&paths), s);
        let _ = fs::remove_dir_all(paths.root());
    }

    #[test]
    fn corrupt_file_falls_back_and_values_are_clamped() {
        let paths = temp_paths("bad");
        fs::create_dir_all(paths.root()).unwrap();
        fs::write(paths.settings_file(), "{ not json").unwrap();
        assert_eq!(Settings::load(&paths), Settings::default());
        fs::write(paths.settings_file(), r#"{"editor_font_size": 900, "sandbox_timeout_secs": 0}"#).unwrap();
        let s = Settings::load(&paths);
        assert_eq!(s.editor_font_size, 40.0);
        assert_eq!(s.sandbox_timeout_secs, 3);
        let _ = fs::remove_dir_all(paths.root());
    }
}

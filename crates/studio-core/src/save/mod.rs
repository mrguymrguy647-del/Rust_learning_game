//! Versioned JSON save files: four manual slots plus an autosave slot.
//!
//! A save file looks like `{ "version": N, "meta": {…}, "state": {…} }`. Older versions are
//! upgraded by the migration chain in [`migrate`]; saves from a *newer* game version are
//! rejected with a clear message instead of being silently corrupted.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::AppPaths;
use crate::sim::GameState;

/// Bump when the save format changes in a way `#[serde(default)]` cannot absorb,
/// and add a step to [`MIGRATIONS`].
pub const SAVE_VERSION: u32 = 1;

pub const AUTOSAVE_SLOT: &str = "autosave";
pub const MANUAL_SLOTS: [&str; 4] = ["slot1", "slot2", "slot3", "slot4"];

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Json(serde_json::Error),
    /// The file was written by a newer version of the game.
    TooNew {
        found: u32,
        supported: u32,
    },
    /// The file is valid JSON but not a save file.
    Malformed(String),
    /// Slot names are restricted so they can never escape the saves directory.
    BadSlot(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "file error: {e}"),
            SaveError::Json(e) => write!(f, "corrupt save data: {e}"),
            SaveError::TooNew { found, supported } => write!(
                f,
                "this save was made by a newer version of the game (save format {found}, this build understands up to {supported})"
            ),
            SaveError::Malformed(m) => write!(f, "not a valid save file: {m}"),
            SaveError::BadSlot(s) => write!(f, "invalid save slot name `{s}`"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<io::Error> for SaveError {
    fn from(e: io::Error) -> Self {
        SaveError::Io(e)
    }
}

impl From<serde_json::Error> for SaveError {
    fn from(e: serde_json::Error) -> Self {
        SaveError::Json(e)
    }
}

/// Summary shown in the load screen without having to understand the whole state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveMeta {
    pub studio_name: String,
    pub week: u32,
    pub money: i64,
    pub tier: usize,
    pub games_released: usize,
    pub saved_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    pub meta: SaveMeta,
    pub state: GameState,
}

/// Info about one slot on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotInfo {
    pub slot: String,
    pub meta: Option<SaveMeta>,
    /// Set when a file exists but cannot be loaded.
    pub error: Option<String>,
}

impl SlotInfo {
    pub fn is_empty(&self) -> bool {
        self.meta.is_none() && self.error.is_none()
    }
}

type Migration = fn(&mut Value) -> Result<(), SaveError>;

/// `MIGRATIONS[i]` upgrades a save from version `i + 1` to `i + 2`.
const MIGRATIONS: &[Migration] = &[];

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Write `bytes` to `path` atomically (temp file + rename) so a crash never leaves half a save.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn slot_path(paths: &AppPaths, slot: &str) -> Result<std::path::PathBuf, SaveError> {
    let valid = !slot.is_empty()
        && slot.len() <= 32
        && slot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Err(SaveError::BadSlot(slot.to_string()));
    }
    Ok(paths.saves_dir().join(format!("{slot}.json")))
}

pub fn meta_for(state: &GameState) -> SaveMeta {
    SaveMeta {
        studio_name: state.studio.name.clone(),
        week: state.date.week(),
        money: state.studio.money,
        tier: state.studio.tier,
        games_released: 0,
        saved_at_unix: now_unix(),
    }
}

pub fn save_game(paths: &AppPaths, slot: &str, state: &GameState) -> Result<(), SaveError> {
    let path = slot_path(paths, slot)?;
    fs::create_dir_all(paths.saves_dir())?;
    let file = SaveFile { version: SAVE_VERSION, meta: meta_for(state), state: state.clone() };
    let text = serde_json::to_string(&file)?;
    atomic_write(&path, text.as_bytes())?;
    Ok(())
}

/// Upgrade a parsed save through every needed migration step.
pub fn migrate(value: &mut Value) -> Result<(), SaveError> {
    migrate_with(value, MIGRATIONS, SAVE_VERSION)
}

fn migrate_with(value: &mut Value, steps: &[Migration], current: u32) -> Result<(), SaveError> {
    let found = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| SaveError::Malformed("missing `version`".into()))? as u32;
    if found > current {
        return Err(SaveError::TooNew { found, supported: current });
    }
    if found == 0 {
        return Err(SaveError::Malformed("version 0 does not exist".into()));
    }
    for v in found..current {
        let step = steps
            .get((v - 1) as usize)
            .ok_or_else(|| SaveError::Malformed(format!("no migration from version {v}")))?;
        step(value)?;
        value["version"] = Value::from(v + 1);
    }
    Ok(())
}

pub fn parse_save(text: &str) -> Result<SaveFile, SaveError> {
    let mut value: Value = serde_json::from_str(text)?;
    migrate(&mut value)?;
    Ok(serde_json::from_value(value)?)
}

pub fn load_game(paths: &AppPaths, slot: &str) -> Result<SaveFile, SaveError> {
    let text = fs::read_to_string(slot_path(paths, slot)?)?;
    parse_save(&text)
}

pub fn delete_save(paths: &AppPaths, slot: &str) -> Result<(), SaveError> {
    match fs::remove_file(slot_path(paths, slot)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Inspect autosave + manual slots.
pub fn list_slots(paths: &AppPaths) -> Vec<SlotInfo> {
    std::iter::once(AUTOSAVE_SLOT)
        .chain(MANUAL_SLOTS)
        .map(|slot| {
            let exists = slot_path(paths, slot).map(|p| p.exists()).unwrap_or(false);
            if !exists {
                return SlotInfo { slot: slot.to_string(), meta: None, error: None };
            }
            match load_game(paths, slot) {
                Ok(f) => SlotInfo { slot: slot.to_string(), meta: Some(f.meta), error: None },
                Err(e) => SlotInfo { slot: slot.to_string(), meta: None, error: Some(e.to_string()) },
            }
        })
        .collect()
}

/// The most recently written loadable slot (for "Continue").
pub fn latest_slot(paths: &AppPaths) -> Option<String> {
    list_slots(paths)
        .into_iter()
        .filter_map(|s| s.meta.map(|m| (m.saved_at_unix, s.slot)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, slot)| slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;
    use crate::sim::Studio;

    fn temp_paths(name: &str) -> AppPaths {
        let dir = std::env::temp_dir().join(format!("rst_save_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        AppPaths::at(dir)
    }

    #[test]
    fn save_and_load_round_trip() {
        let paths = temp_paths("rt");
        let mut state = GameState::new_game(
            &crate::data::ContentLibrary::embedded(),
            "Ferris Games",
            "Sam",
            Difficulty::Hard,
            Some(5),
        );
        state.studio.money = 12_345;
        save_game(&paths, "slot1", &state).unwrap();
        let loaded = load_game(&paths, "slot1").unwrap();
        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.state, state);
        assert_eq!(loaded.meta.studio_name, "Ferris Games");
        assert_eq!(loaded.meta.money, 12_345);
        let _ = fs::remove_dir_all(paths.root());
    }

    #[test]
    fn slot_names_cannot_escape_the_saves_dir() {
        let paths = temp_paths("slot");
        let state = GameState::default();
        for bad in ["../evil", "a/b", "", "x y", "..", "a\\b"] {
            assert!(matches!(save_game(&paths, bad, &state), Err(SaveError::BadSlot(_))), "{bad}");
        }
        let _ = fs::remove_dir_all(paths.root());
    }

    #[test]
    fn newer_saves_are_rejected_clearly() {
        let text = format!(r#"{{"version": {}, "meta": {{}}, "state": {{}}}}"#, SAVE_VERSION + 1);
        match parse_save(&text) {
            Err(SaveError::TooNew { found, supported }) => {
                assert_eq!((found, supported), (SAVE_VERSION + 1, SAVE_VERSION));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let text = format!(
            r#"{{"version": {SAVE_VERSION}, "meta": {{}}, "state": {{"studio": {{"name": "Old"}}}}}}"#
        );
        let f = parse_save(&text).unwrap();
        assert_eq!(f.state.studio.name, "Old");
        assert_eq!(f.state.studio.money, Studio::default().money);
    }

    #[test]
    fn migrations_run_in_order() {
        fn v1_to_v2(v: &mut Value) -> Result<(), SaveError> {
            v["state"]["studio"]["name"] = Value::from("migrated");
            Ok(())
        }
        fn v2_to_v3(v: &mut Value) -> Result<(), SaveError> {
            let name = v["state"]["studio"]["name"].as_str().unwrap_or("").to_string();
            v["state"]["studio"]["name"] = Value::from(format!("{name}+v3"));
            Ok(())
        }
        let mut value: Value = serde_json::from_str(r#"{"version":1,"state":{"studio":{}}}"#).unwrap();
        migrate_with(&mut value, &[v1_to_v2, v2_to_v3], 3).unwrap();
        assert_eq!(value["version"], 3);
        assert_eq!(value["state"]["studio"]["name"], "migrated+v3");
        assert!(migrate_with(&mut value, &[], 5).is_err(), "missing migration step must error");
    }

    #[test]
    fn list_slots_reports_corrupt_files() {
        let paths = temp_paths("list");
        save_game(&paths, AUTOSAVE_SLOT, &GameState::default()).unwrap();
        fs::write(paths.saves_dir().join("slot2.json"), "garbage").unwrap();
        let slots = list_slots(&paths);
        assert_eq!(slots.len(), 5);
        assert!(slots[0].meta.is_some());
        assert!(slots[2].error.is_some());
        assert!(slots[1].is_empty());
        assert_eq!(latest_slot(&paths).as_deref(), Some(AUTOSAVE_SLOT));
        let _ = fs::remove_dir_all(paths.root());
    }
}

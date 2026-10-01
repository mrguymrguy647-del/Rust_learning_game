//! The player's engine: which modules are built, which one is being researched, and what they
//! provide (quality bonuses and feature tags that unlock genres, sizes and platforms).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::model::{Category, Weights};
use crate::data::{ContentLibrary, EngineModuleDef};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Build {
    pub module: String,
    pub weeks_left: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineState {
    pub built: BTreeSet<String>,
    pub building: Option<Build>,
}

/// Why a module cannot be started yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    AlreadyBuilt,
    Building,
    Tier(usize),
    Module(String),
    Challenge(String),
    ResearchPoints { have: i64, need: i64 },
    Money { have: i64, need: i64 },
    OtherBuildInProgress,
}

impl EngineState {
    /// A new studio starts with the modules flagged `starts_built`.
    pub fn initial(content: &ContentLibrary) -> EngineState {
        EngineState {
            built: content.engine_modules.iter().filter(|m| m.starts_built).map(|m| m.id.clone()).collect(),
            building: None,
        }
    }

    pub fn has_module(&self, id: &str) -> bool {
        self.built.contains(id)
    }

    pub fn modules<'a>(&'a self, content: &'a ContentLibrary) -> impl Iterator<Item = &'a EngineModuleDef> {
        content.engine_modules.iter().filter(|m| self.built.contains(&m.id))
    }

    pub fn has_feature(&self, content: &ContentLibrary, feature: &str) -> bool {
        self.modules(content).any(|m| m.features.iter().any(|f| f == feature))
    }

    pub fn has_all_features(&self, content: &ContentLibrary, features: &[String]) -> bool {
        features.iter().all(|f| self.has_feature(content, f))
    }

    /// First feature in `features` that is missing, if any.
    pub fn missing_feature<'a>(
        &self,
        content: &ContentLibrary,
        features: &'a [String],
    ) -> Option<&'a String> {
        features.iter().find(|f| !self.has_feature(content, f))
    }

    /// Total additive quality bonus for a category from every built module.
    pub fn bonus(&self, content: &ContentLibrary, c: Category) -> f32 {
        self.modules(content).map(|m| m.bonus.get(c)).sum()
    }

    pub fn bonuses(&self, content: &ContentLibrary) -> Weights {
        Weights::default().map(|c, _| self.bonus(content, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::model::Weights;

    fn lib_with_modules() -> ContentLibrary {
        let mut lib = ContentLibrary::default();
        let module = |id: &str, features: &[&str], gfx: f32, built: bool| EngineModuleDef {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            topic: "basics".into(),
            requires_modules: vec![],
            required_challenges: vec![],
            research_cost: 10,
            money_cost: 100,
            build_weeks: 2,
            features: features.iter().map(|s| s.to_string()).collect(),
            bonus: Weights { graphics: gfx, ..Weights::default() },
            min_tier: 0,
            starts_built: built,
        };
        lib.engine_modules =
            vec![module("core", &["core"], 0.0, true), module("r2d", &["renderer_2d"], 0.2, false)];
        lib
    }

    #[test]
    fn starting_modules_and_features() {
        let lib = lib_with_modules();
        let mut e = EngineState::initial(&lib);
        assert!(e.has_module("core") && !e.has_module("r2d"));
        assert!(e.has_feature(&lib, "core") && !e.has_feature(&lib, "renderer_2d"));
        assert_eq!(
            e.missing_feature(&lib, &["core".into(), "renderer_2d".into()]).map(String::as_str),
            Some("renderer_2d")
        );
        e.built.insert("r2d".into());
        assert!(e.has_all_features(&lib, &["core".into(), "renderer_2d".into()]));
        assert!((e.bonus(&lib, Category::Graphics) - 0.2).abs() < 1e-6);
        assert_eq!(e.bonus(&lib, Category::Audio), 0.0);
    }
}

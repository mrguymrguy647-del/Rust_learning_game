//! Non-challenge game data: topics, studio tiers, codex, compiler-error explanations, …
//! All of it lives in `content/game/*.ron` and `content/codex/*.ron`.

use serde::{Deserialize, Serialize};

/// Implemented by every content type that is keyed by an id (used for merging/overriding).
pub trait Identified {
    fn id(&self) -> &str;
}

macro_rules! identified {
    ($($t:ty),* $(,)?) => { $(impl Identified for $t { fn id(&self) -> &str { &self.id } })* };
}

/// One curriculum topic (Ownership, Traits, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicDef {
    pub id: String,
    /// 1-based position in the curriculum.
    pub order: u32,
    pub name: String,
    /// The engine/game feature this topic is themed around ("Asset manager").
    pub feature_name: String,
    pub summary: String,
    /// Link to the matching chapter(s) of The Rust Book.
    #[serde(default)]
    pub book_url: String,
}

/// A studio tier (Bedroom … AAA HQ).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Weekly rent in dollars.
    pub rent: i64,
    /// Max employees including the founder.
    pub staff_slots: usize,
    /// One-off cost of moving in.
    pub upgrade_cost: i64,
    /// Reputation (fans) needed before upgrading to this tier.
    pub required_reputation: i64,
    /// Highest challenge difficulty (1..=5) that will be triggered at this tier.
    pub max_difficulty: u8,
    /// Maximum total loan principal available.
    pub loan_limit: i64,
    /// How many studios can license your engine at once.
    pub license_slots: usize,
}

/// One entry of the in-game Rust encyclopedia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexEntry {
    pub id: String,
    pub topic: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub book_url: String,
    /// Unlocked once this challenge is solved. If `None`, unlocked when the topic is started.
    #[serde(default)]
    pub unlock_challenge: Option<String>,
}

/// Friendly beginner explanation for a rustc error code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorExplainer {
    /// e.g. `E0382`
    pub id: String,
    pub title: String,
    pub explanation: String,
    pub how_to_fix: String,
    #[serde(default)]
    pub book_url: String,
}

identified!(TopicDef, TierDef, CodexEntry, ErrorExplainer);

/// An achievement unlocked when a named statistic reaches a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AchievementDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Key into `PlayerProgress::stats` (e.g. `challenges_solved`).
    pub stat: String,
    pub at_least: i64,
    #[serde(default)]
    pub xp: u32,
}

identified!(AchievementDef);

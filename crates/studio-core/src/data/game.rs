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

/// A game genre. Decides the ideal focus split and which engine features are needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenreDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Ideal focus distribution (sums to 100).
    pub ideal: crate::sim::model::Weights,
    /// Engine features (see engine modules) the studio must have built.
    #[serde(default)]
    pub requires_features: Vec<String>,
    /// Base popularity multiplier (before market trends).
    pub popularity: f32,
    /// Demand multiplier for `[Everyone, Teen, Mature]`.
    pub audience_fit: Vec<f32>,
    /// Theme compatibility multipliers (`theme id`, factor). Missing = 1.0.
    #[serde(default)]
    pub theme_fit: Vec<(String, f32)>,
}

impl GenreDef {
    /// Demand multiplier for an audience (`0` = Everyone, `1` = Teen, `2` = Mature).
    pub fn audience_factor(&self, audience_index: usize) -> f32 {
        self.audience_fit.get(audience_index).copied().unwrap_or(1.0)
    }

    pub fn theme_factor(&self, theme: &str) -> f32 {
        self.theme_fit.iter().find(|(t, _)| t == theme).map(|(_, f)| *f).unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub flavor: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformKind {
    Pc,
    Console,
    Handheld,
    Mobile,
}

/// A platform with a life cycle: it launches, peaks and fades.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformDef {
    pub id: String,
    pub name: String,
    pub kind: PlatformKind,
    #[serde(default)]
    pub description: String,
    /// Absolute game week it becomes available.
    pub launch_week: u32,
    /// Week of peak popularity. Ignored for evergreen platforms.
    #[serde(default)]
    pub peak_week: u32,
    /// Week it is discontinued (market collapses to a tail). `0` = evergreen.
    #[serde(default)]
    pub end_week: u32,
    /// Size of the audience at the peak, relative to a reference PC market of 1.0.
    pub market: f32,
    /// Evergreen platforms grow by this fraction every game year.
    #[serde(default)]
    pub growth_per_year: f32,
    /// Share of revenue the platform keeps.
    pub store_cut: f32,
    /// Prices are multiplied by this (mobile games are cheap).
    pub price_mult: f32,
    /// One-off dev-kit / licence fee per project.
    #[serde(default)]
    pub dev_cost: i64,
    #[serde(default)]
    pub requires_features: Vec<String>,
    /// Minimum studio tier index.
    #[serde(default)]
    pub min_tier: usize,
}

impl PlatformDef {
    pub fn is_evergreen(&self) -> bool {
        self.end_week == 0
    }

    pub fn available(&self, week: u32) -> bool {
        week >= self.launch_week && (self.is_evergreen() || week < self.end_week + 52)
    }

    /// Relative market size at `week` (0 before launch).
    pub fn market_at(&self, week: u32) -> f32 {
        if week < self.launch_week {
            return 0.0;
        }
        if self.is_evergreen() {
            let years = (week - self.launch_week) as f32 / 52.0;
            return self.market * (1.0 + self.growth_per_year * years);
        }
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
        let w = week as f32;
        let curve = if week < self.peak_week {
            lerp(
                0.15,
                1.0,
                (w - self.launch_week as f32) / ((self.peak_week - self.launch_week).max(1)) as f32,
            )
        } else if week < self.end_week {
            lerp(1.0, 0.25, (w - self.peak_week as f32) / ((self.end_week - self.peak_week).max(1)) as f32)
        } else {
            0.1
        };
        self.market * curve
    }

    /// Lifecycle label for the UI.
    pub fn stage(&self, week: u32) -> &'static str {
        if week < self.launch_week {
            "upcoming"
        } else if self.is_evergreen() {
            "evergreen"
        } else if week < self.peak_week {
            "growing"
        } else if week < self.end_week {
            "declining"
        } else {
            "discontinued"
        }
    }
}

/// A fictional review outlet with its own tastes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutletDef {
    pub id: String,
    pub name: String,
    pub tagline: String,
    /// What this outlet cares about (sums to 1).
    pub weights: crate::sim::model::Weights,
    /// Added to the 10-point score (negative = harsh critic).
    pub harshness: f32,
    /// Quote templates. Placeholders: `{game}`, `{strong}`, `{weak}`, `{genre}`.
    pub quotes_great: Vec<String>,
    pub quotes_good: Vec<String>,
    pub quotes_mixed: Vec<String>,
    pub quotes_bad: Vec<String>,
    /// Extra lines used when the game is buggy.
    pub quotes_buggy: Vec<String>,
}

/// One project size tier and its economics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizeDef {
    pub size: crate::sim::model::ProjectSize,
    /// Work units needed (one person at average speed produces ~10 per week).
    pub work: f32,
    /// Retail price (PC).
    pub price: f32,
    /// Launch-week unit demand scale.
    pub base_demand: f32,
    /// One-off development budget (assets, licences, servers), spent in proportion to progress.
    pub budget: i64,
    pub min_tier: usize,
    /// How many blocking challenges appear during development.
    pub blockers: u32,
    /// Quality a game of this size is expected to reach (0..1) before reviewers are impressed.
    pub expectation: f32,
    #[serde(default)]
    pub requires_features: Vec<String>,
}

/// Tunable numbers of the simulation (`game/balance.ron`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Balance {
    pub start_money: i64,
    pub sizes: Vec<SizeDef>,
    /// Weeks of negative cash before the studio goes bankrupt.
    pub bankruptcy_weeks: u32,
    /// Work multiplier while crunching.
    pub crunch_speed: f32,
    /// Morale lost per week of crunch.
    pub crunch_morale_loss: f32,
    /// Bugs added per week of crunch (percent points).
    pub crunch_bugs: f32,
    /// Base weekly bug growth per unit of work (percent points per 10 work).
    pub bug_rate: f32,
    /// Weekly sales decay at a metascore of 0 / 100.
    pub decay_low: f32,
    pub decay_high: f32,
    /// Sales stop when weekly units fall below this.
    pub min_weekly_units: f32,
    /// Interest on loans, per week (0.004 = 0.4 %).
    pub loan_weekly_rate: f32,
    /// Recruiting fee as a multiple of the weekly salary.
    pub hire_fee_weeks: f32,
    pub candidate_refresh_weeks: u32,
    /// Research points per week per programming skill point above 2.
    pub research_per_skill_point: f32,
}

impl Default for Balance {
    fn default() -> Self {
        use crate::sim::model::ProjectSize::*;
        let size = |size, work, price, base_demand, budget, min_tier, blockers, expectation| SizeDef {
            size,
            work,
            price,
            base_demand,
            budget,
            min_tier,
            blockers,
            expectation,
            requires_features: Vec::new(),
        };
        Balance {
            start_money: 15_000,
            sizes: vec![
                size(Small, 220.0, 9.99, 1500.0, 2_000, 0, 2, 0.75),
                size(Medium, 800.0, 19.99, 4000.0, 15_000, 1, 3, 0.88),
                size(Large, 2600.0, 29.99, 9_000.0, 90_000, 2, 4, 1.02),
                size(AAA, 8000.0, 49.99, 40_000.0, 600_000, 3, 6, 1.6),
            ],
            bankruptcy_weeks: 8,
            crunch_speed: 1.35,
            crunch_morale_loss: 6.0,
            crunch_bugs: 1.2,
            bug_rate: 0.35,
            decay_low: 0.55,
            decay_high: 0.85,
            min_weekly_units: 5.0,
            loan_weekly_rate: 0.004,
            hire_fee_weeks: 2.0,
            candidate_refresh_weeks: 4,
            research_per_skill_point: 0.35,
        }
    }
}

impl Balance {
    /// The economics of `size`. Falls back to the built-in defaults if a content override
    /// forgot to define it, so the simulation can never fail on a missing entry.
    pub fn size(&self, size: crate::sim::model::ProjectSize) -> SizeDef {
        self.sizes
            .iter()
            .find(|s| s.size == size)
            .cloned()
            .or_else(|| Balance::default().sizes.into_iter().find(|s| s.size == size))
            .unwrap_or_else(|| Balance::default().sizes.remove(0))
    }
}

identified!(GenreDef, ThemeDef, PlatformDef, OutletDef);

/// An engine module the studio can build. Needs research points, money and specific challenges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineModuleDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Curriculum topic this module is related to.
    pub topic: String,
    /// Other modules that must be built first.
    #[serde(default)]
    pub requires_modules: Vec<String>,
    /// Challenges that must be solved before the module can be built.
    #[serde(default)]
    pub required_challenges: Vec<String>,
    /// `(topic id, count)`: this many challenges of the topic must be solved.
    #[serde(default)]
    pub required_topic_solves: Vec<(String, u32)>,
    /// Research points spent when starting the build.
    pub research_cost: i64,
    pub money_cost: i64,
    pub build_weeks: u32,
    /// Feature tags this module provides (checked by genres, sizes and platforms).
    #[serde(default)]
    pub features: Vec<String>,
    /// Additive quality multipliers (e.g. `graphics: 0.15` = +15 % effective graphics skill).
    #[serde(default)]
    pub bonus: crate::sim::model::Weights,
    #[serde(default)]
    pub min_tier: usize,
    /// Already built when a new game starts.
    #[serde(default)]
    pub starts_built: bool,
}

identified!(EngineModuleDef);

/// A staff personality trait with small mechanical effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub output_mult: f32,
    /// Added to the owner's morale every week.
    pub morale_delta: f32,
    pub bug_mult: f32,
    /// Multiplier on training duration (below 1 = faster).
    pub training_mult: f32,
    /// Gives the studio one extra free hint (like a senior developer).
    pub free_hint: bool,
    /// Added to every other team member's morale each week.
    pub team_morale: f32,
}

/// Name pools for generated job candidates.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NamePool {
    pub first: Vec<String>,
    pub last: Vec<String>,
}

identified!(TraitDef);

/// A rival studio that releases games and competes for awards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompetitorDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    /// 0..1: how good their games usually are.
    pub strength: f32,
    pub genres: Vec<String>,
    /// Average weeks between releases.
    pub interval_weeks: u32,
}

/// A marketing campaign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketingDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost: i64,
    pub hype: f32,
    #[serde(default)]
    pub min_tier: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Decision,
    Crunch,
    Conference,
    Publisher,
    GameJam,
    Hotfix,
}

/// Conditions that make an event eligible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Cond {
    HasProject,
    NoProject,
    HasReleasedGame,
    NoContract,
    MinMoney(i64),
    MinFans(i64),
    MinStaff(u32),
}

/// Immediate consequences of a choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    Money(i64),
    Fans(i64),
    /// Added to every employee's morale.
    Morale(f32),
    /// Added to the current project's hype.
    Hype(f32),
    /// Fraction of the current project's total work added (negative = lost).
    Work(f32),
    CrunchBugs(f32),
    Research(i64),
    Xp(u32),
    /// Change the bug level of the event's target game.
    TargetBugs(f32),
    /// Multiply the target game's sales.
    TargetSalesMult(f32),
}

/// Special behaviour of a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChoiceAction {
    #[default]
    None,
    /// Open a (blocking) hotfix challenge for the target game.
    StartHotfix,
    /// Open a timed jam challenge.
    StartJam,
    /// Accept the event's publisher contract.
    AcceptContract,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventChoice {
    pub label: String,
    #[serde(default)]
    pub detail: String,
    /// Money the player must pay to pick this option.
    #[serde(default)]
    pub cost: i64,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub action: ChoiceAction,
}

/// A publisher contract offered by an event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractTemplate {
    pub publisher: String,
    /// Required genre id; empty = any genre.
    #[serde(default)]
    pub genre: String,
    pub min_size: crate::sim::model::ProjectSize,
    pub weeks: u32,
    pub advance: i64,
    pub bonus: i64,
    pub min_meta: f32,
}

fn one() -> f32 {
    1.0
}

fn any_tier() -> usize {
    99
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDef {
    pub id: String,
    pub kind: EventKind,
    pub title: String,
    pub text: String,
    #[serde(default = "one")]
    pub weight: f32,
    #[serde(default)]
    pub min_tier: usize,
    #[serde(default = "any_tier")]
    pub max_tier: usize,
    #[serde(default)]
    pub min_week: u32,
    #[serde(default)]
    pub requires: Vec<Cond>,
    #[serde(default)]
    pub cooldown_weeks: u32,
    pub choices: Vec<EventChoice>,
    #[serde(default)]
    pub contract: Option<ContractTemplate>,
    /// Time limit of a jam challenge, in minutes.
    #[serde(default)]
    pub jam_minutes: u32,
    #[serde(default)]
    pub jam_prize: i64,
    #[serde(default)]
    pub jam_fans: i64,
}

identified!(CompetitorDef, MarketingDef, EventDef);

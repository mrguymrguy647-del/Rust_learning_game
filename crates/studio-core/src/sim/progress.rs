//! The player's Rust learning progress: XP/level, solved challenges, topic mastery,
//! statistics, achievements and codex unlocks.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::data::{AchievementDef, Challenge, CodexEntry, ContentLibrary};

pub const MAX_LEVEL: u32 = 20;

/// Share of the previous topic that must be mastered before the next topic opens.
pub const TOPIC_UNLOCK_THRESHOLD: f32 = 0.6;

/// Whether the player may start a challenge right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Solved,
    Available,
    /// An earlier challenge in the same topic must be solved first.
    LockedPrerequisite,
    /// The previous topic is not mastered enough yet.
    LockedTopic,
}

/// What the player did the last time they solved a challenge.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SolveRecord {
    /// Submissions (compile & test runs) in the session that solved it.
    pub attempts: u32,
    pub first_try: bool,
    pub hints_used: u32,
    /// A contractor solved it (counts much less towards mastery).
    pub contractor: bool,
    pub solved_week: u32,
    /// How many times it was solved (study/engine/project; practice not counted).
    pub times_solved: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerProgress {
    pub xp: u32,
    pub solved: BTreeMap<String, SolveRecord>,
    pub stats: BTreeMap<String, i64>,
    pub achievements: BTreeSet<String>,
    /// Codex entries the player has opened (to show a "new" badge).
    pub codex_seen: BTreeSet<String>,
}

/// Cumulative XP needed to *reach* `level` (level 1 needs 0).
pub fn xp_for_level(level: u32) -> u32 {
    let n = level.clamp(1, MAX_LEVEL + 1) - 1;
    30 * n * (n + 1)
}

pub fn level_for_xp(xp: u32) -> u32 {
    let mut level = 1;
    while level < MAX_LEVEL && xp >= xp_for_level(level + 1) {
        level += 1;
    }
    level
}

pub fn level_title(level: u32) -> &'static str {
    match level {
        0..=2 => "Hello, World Hacker",
        3..=4 => "Borrow-Checker Apprentice",
        5..=6 => "Ownership Enthusiast",
        7..=8 => "Trait Tinkerer",
        9..=10 => "Lifetime Wrangler",
        11..=12 => "Fearless Concurrency Cadet",
        13..=14 => "Macro Magician",
        15..=17 => "Unsafe Whisperer",
        _ => "Rustacean Sage",
    }
}

impl PlayerProgress {
    pub fn level(&self) -> u32 {
        level_for_xp(self.xp)
    }

    /// `(xp into this level, xp needed for the next level)`; `None` needed at max level.
    pub fn level_progress(&self) -> (u32, Option<u32>) {
        let level = self.level();
        let base = xp_for_level(level);
        if level >= MAX_LEVEL {
            (self.xp - base, None)
        } else {
            (self.xp - base, Some(xp_for_level(level + 1) - base))
        }
    }

    pub fn stat(&self, key: &str) -> i64 {
        self.stats.get(key).copied().unwrap_or(0)
    }

    pub fn bump(&mut self, key: &str, by: i64) {
        *self.stats.entry(key.to_string()).or_insert(0) += by;
    }

    pub fn set_max(&mut self, key: &str, value: i64) {
        let e = self.stats.entry(key.to_string()).or_insert(0);
        *e = (*e).max(value);
    }

    pub fn is_solved(&self, id: &str) -> bool {
        self.solved.contains_key(id)
    }

    /// All prerequisites solved?
    pub fn prerequisites_met(&self, c: &Challenge) -> bool {
        c.prerequisites.iter().all(|p| self.is_solved(p))
    }

    pub fn solved_in_topic(&self, content: &ContentLibrary, topic: &str) -> usize {
        content.challenges_in_topic(topic).filter(|c| self.is_solved(&c.id)).count()
    }

    /// 0..=1: difficulty-weighted share of the topic that was genuinely learned.
    /// Contractor-solved challenges count 40 %.
    pub fn mastery(&self, content: &ContentLibrary, topic: &str) -> f32 {
        let (mut got, mut total) = (0.0f32, 0.0f32);
        for c in content.challenges_in_topic(topic) {
            let w = c.difficulty.clamp(1, 5) as f32;
            total += w;
            if let Some(rec) = self.solved.get(&c.id) {
                got += w * if rec.contractor { 0.4 } else { 1.0 };
            }
        }
        if total <= 0.0 {
            0.0
        } else {
            (got / total).clamp(0.0, 1.0)
        }
    }

    /// A topic opens once the previous topic is at least `threshold` mastered
    /// (the first topic is always open).
    pub fn topic_unlocked(&self, content: &ContentLibrary, topic_index: usize, threshold: f32) -> bool {
        if topic_index == 0 {
            return true;
        }
        match content.topics.get(topic_index - 1) {
            Some(prev) => {
                // A topic with no challenges can never block the curriculum.
                content.challenges_in_topic(&prev.id).next().is_none()
                    || self.mastery(content, &prev.id) >= threshold
            }
            None => true,
        }
    }

    pub fn availability(&self, content: &ContentLibrary, c: &Challenge) -> Availability {
        if self.is_solved(&c.id) {
            return Availability::Solved;
        }
        let topic_index = content.topics.iter().position(|t| t.id == c.topic).unwrap_or(0);
        if !self.topic_unlocked(content, topic_index, TOPIC_UNLOCK_THRESHOLD) {
            Availability::LockedTopic
        } else if !self.prerequisites_met(c) {
            Availability::LockedPrerequisite
        } else {
            Availability::Available
        }
    }

    pub fn codex_unlocked(&self, entry: &CodexEntry) -> bool {
        match &entry.unlock_challenge {
            None => true,
            Some(id) => self.is_solved(id),
        }
    }

    /// Unlocked entries the player has not opened yet.
    pub fn codex_new_count(&self, codex: &[CodexEntry]) -> usize {
        codex.iter().filter(|e| self.codex_unlocked(e) && !self.codex_seen.contains(&e.id)).count()
    }

    pub fn mark_codex_seen(&mut self, id: &str) {
        if !self.codex_seen.contains(id) {
            self.codex_seen.insert(id.to_string());
        }
    }

    /// Unlock newly satisfied achievements, award their XP and return them.
    pub fn check_achievements<'a>(&mut self, defs: &'a [AchievementDef]) -> Vec<&'a AchievementDef> {
        let mut unlocked = Vec::new();
        for def in defs {
            if !self.achievements.contains(&def.id) && self.stat(&def.stat) >= def.at_least {
                self.achievements.insert(def.id.clone());
                self.xp += def.xp;
                unlocked.push(def);
            }
        }
        unlocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_curve_is_monotonic_and_capped() {
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(59), 1);
        assert_eq!(level_for_xp(60), 2);
        let mut prev = 0;
        for l in 1..=MAX_LEVEL + 1 {
            let x = xp_for_level(l);
            assert!(x >= prev);
            prev = x;
        }
        assert_eq!(level_for_xp(u32::MAX / 2), MAX_LEVEL);
    }

    #[test]
    fn level_progress_reports_remaining_xp() {
        let p = PlayerProgress { xp: 100, ..Default::default() };
        assert_eq!(p.level(), 2);
        assert_eq!(p.level_progress(), (40, Some(120)));
        let max = PlayerProgress { xp: xp_for_level(MAX_LEVEL) + 5, ..Default::default() };
        assert_eq!(max.level_progress(), (5, None));
    }

    #[test]
    fn achievements_unlock_once_and_award_xp() {
        let defs = vec![AchievementDef {
            id: "ten".into(),
            name: "Ten".into(),
            description: String::new(),
            stat: "challenges_solved".into(),
            at_least: 10,
            xp: 50,
        }];
        let mut p = PlayerProgress::default();
        p.bump("challenges_solved", 9);
        assert!(p.check_achievements(&defs).is_empty());
        p.bump("challenges_solved", 1);
        assert_eq!(p.check_achievements(&defs).len(), 1);
        assert_eq!(p.xp, 50);
        assert!(p.check_achievements(&defs).is_empty(), "must only unlock once");
        assert_eq!(p.xp, 50);
    }

    #[test]
    fn mastery_discounts_contractor_solves() {
        let lib = ContentLibrary::embedded();
        let topic = "basics";
        let mut p = PlayerProgress::default();
        assert_eq!(p.mastery(&lib, topic), 0.0);
        for c in lib.challenges_in_topic(topic) {
            p.solved.insert(c.id.clone(), SolveRecord { contractor: true, ..Default::default() });
        }
        assert!((p.mastery(&lib, topic) - 0.4).abs() < 1e-4);
        for rec in p.solved.values_mut() {
            rec.contractor = false;
        }
        assert!((p.mastery(&lib, topic) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn availability_follows_prerequisites_and_topic_order() {
        let lib = ContentLibrary::embedded();
        let mut p = PlayerProgress::default();
        let first = &lib.challenges_in_topic("basics").next().unwrap().clone();
        assert_eq!(p.availability(&lib, first), Availability::Available);
        let second = lib.challenges_in_topic("basics").nth(1).unwrap().clone();
        assert_eq!(p.availability(&lib, &second), Availability::LockedPrerequisite);
        let other_topic = lib.challenges_in_topic("ownership").next().unwrap().clone();
        assert_eq!(p.availability(&lib, &other_topic), Availability::LockedTopic);
        p.solved.insert(first.id.clone(), SolveRecord::default());
        assert_eq!(p.availability(&lib, first), Availability::Solved);
        assert_eq!(p.availability(&lib, &second), Availability::Available);
    }

    #[test]
    fn first_topic_is_open_next_needs_mastery() {
        let lib = ContentLibrary::embedded();
        let mut p = PlayerProgress::default();
        assert!(p.topic_unlocked(&lib, 0, 0.6));
        assert!(!p.topic_unlocked(&lib, 1, 0.6));
        for c in lib.challenges_in_topic(&lib.topics[0].id) {
            p.solved.insert(c.id.clone(), SolveRecord::default());
        }
        assert!(p.topic_unlocked(&lib, 1, 0.6));
    }
}

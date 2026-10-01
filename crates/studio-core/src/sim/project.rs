//! A game in development: configuration, weekly progress, blocking challenges and release.

use serde::{Deserialize, Serialize};

use super::challenge_flow::{Attempt, ChallengeContext, RewardSummary};
use super::library::ReleasedGame;
use super::model::{Audience, Category, Phase, ProjectSize, Weights};
use super::notify::NoteKind;
use super::progress::Availability;
use super::quality::{self, estimate_bugs};
use super::reviews;
use super::sales::{self, DemandInput};
use super::state::GameState;
use crate::data::ContentLibrary;

/// A development problem that stops the team until a challenge is solved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocker {
    /// Fraction of total work at which it appears.
    pub at: f32,
    pub resolved: bool,
    pub challenge_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: u32,
    pub name: String,
    pub genre: String,
    pub theme: String,
    pub platform: String,
    pub audience: Audience,
    pub size: ProjectSize,
    pub focus: Weights,
    pub started_week: u32,
    pub work_done: f32,
    pub work_total: f32,
    /// Σ (effective skill × work) per category; divide by `output_acc` for the average.
    pub effective_acc: Weights,
    pub output_acc: f32,
    pub blockers: Vec<Blocker>,
    pub skipped_blockers: u32,
    /// Quality bonus collected from solved blockers.
    pub challenge_quality: f32,
    pub crunch: bool,
    pub crunch_weeks: u32,
    pub crunch_bugs: f32,
    pub hype: f32,
    pub budget_total: i64,
    pub budget_spent: i64,
    /// Everything spent on this project (salaries, rent, budget) for profit reporting.
    pub cost_so_far: i64,
    pub weeks_in_dev: u32,
    pub sequel_of: Option<u32>,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            id: 0,
            name: String::new(),
            genre: String::new(),
            theme: String::new(),
            platform: "pc".into(),
            audience: Audience::Everyone,
            size: ProjectSize::Small,
            focus: Weights::EVEN,
            started_week: 0,
            work_done: 0.0,
            work_total: 1.0,
            effective_acc: Weights::default(),
            output_acc: 0.0,
            blockers: Vec::new(),
            skipped_blockers: 0,
            challenge_quality: 0.0,
            crunch: false,
            crunch_weeks: 0,
            crunch_bugs: 0.0,
            hype: 0.0,
            budget_total: 0,
            budget_spent: 0,
            cost_so_far: 0,
            weeks_in_dev: 0,
            sequel_of: None,
        }
    }
}

impl Project {
    pub fn fraction(&self) -> f32 {
        (self.work_done / self.work_total.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn phase(&self) -> Phase {
        Phase::for_fraction(self.fraction())
    }

    pub fn is_complete(&self) -> bool {
        self.work_done >= self.work_total - 0.001
    }

    /// Average effective skill (0..~1.5) per category over all work done so far.
    pub fn avg_effective(&self) -> Weights {
        if self.output_acc <= 0.0 {
            return Weights::default();
        }
        self.effective_acc.map(|_, v| v / self.output_acc)
    }

    /// Index of the first unresolved blocker that is due.
    pub fn due_blocker(&self) -> Option<usize> {
        let f = self.fraction();
        self.blockers.iter().position(|b| !b.resolved && b.challenge_id.is_none() && f >= b.at)
    }

    pub fn open_blockers(&self) -> usize {
        self.blockers.iter().filter(|b| !b.resolved).count()
    }
}

/// What the player chooses when starting a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectConfig {
    pub name: String,
    pub genre: String,
    pub theme: String,
    pub platform: String,
    pub audience: Audience,
    pub size: ProjectSize,
    pub focus: Weights,
    pub sequel_of: Option<u32>,
}

/// The team's output and effective skills for one week.
pub struct TeamWeek {
    pub output: f32,
    /// Effective skill per category (0..~1.5).
    pub effective: Weights,
    pub avg_morale: f32,
}

impl GameState {
    pub fn team_week(&self, content: &ContentLibrary, crunch: bool) -> TeamWeek {
        let crunch_speed = content.balance.crunch_speed;
        let mut total = 0.0;
        let mut acc = Weights::default();
        let mut morale = 0.0;
        for s in &self.staff {
            let out = s.output(crunch, crunch_speed);
            total += out;
            morale += s.morale;
            for c in Category::ALL {
                acc.set(c, acc.get(c) + out * s.category_skill(c));
            }
        }
        let n = self.staff.len().max(1) as f32;
        let effective = if total > 0.0 {
            acc.map(|c, v| v / total / 10.0 * (1.0 + self.engine.bonus(content, c)))
        } else {
            Weights::default()
        };
        TeamWeek { output: total, effective, avg_morale: morale / n }
    }

    /// Why the project cannot be started, or `None` if everything is fine.
    pub fn project_start_problem(&self, content: &ContentLibrary, cfg: &ProjectConfig) -> Option<String> {
        if self.project.is_some() {
            return Some("You are already working on a project.".into());
        }
        if self.game_over.is_some() {
            return Some("The studio has closed.".into());
        }
        let Some(genre) = content.genres.iter().find(|g| g.id == cfg.genre) else {
            return Some("Choose a genre.".into());
        };
        if content.themes.iter().all(|t| t.id != cfg.theme) {
            return Some("Choose a theme.".into());
        }
        let Some(platform) = content.platforms.iter().find(|p| p.id == cfg.platform) else {
            return Some("Choose a platform.".into());
        };
        let size = content.balance.size(cfg.size);
        if self.studio.tier < size.min_tier {
            let tier = content.tier(size.min_tier).map(|t| t.name.as_str()).unwrap_or("a bigger studio");
            return Some(format!("{} projects need {tier}.", cfg.size.label()));
        }
        if let Some(f) = self.engine.missing_feature(content, &size.requires_features) {
            return Some(format!("{} projects need the `{f}` engine feature.", cfg.size.label()));
        }
        if let Some(f) = self.engine.missing_feature(content, &genre.requires_features) {
            return Some(format!("{} games need the `{f}` engine feature.", genre.name));
        }
        if !platform.available(self.date.week()) {
            return Some(format!("{} is not available (yet).", platform.name));
        }
        if self.studio.tier < platform.min_tier {
            return Some(format!("{} needs a bigger studio.", platform.name));
        }
        if let Some(f) = self.engine.missing_feature(content, &platform.requires_features) {
            return Some(format!("{} needs the `{f}` engine feature.", platform.name));
        }
        if self.studio.money < platform.dev_cost {
            return Some(format!(
                "The {} dev kit costs {}.",
                platform.name,
                crate::fmt::money(platform.dev_cost)
            ));
        }
        if (cfg.focus.sum() - 100.0).abs() > 1.0 {
            return Some("Focus must add up to 100%.".into());
        }
        None
    }

    pub fn start_project(&mut self, content: &ContentLibrary, cfg: ProjectConfig) -> Result<(), String> {
        if let Some(problem) = self.project_start_problem(content, &cfg) {
            return Err(problem);
        }
        let size = content.balance.size(cfg.size);
        let platform_cost =
            content.platforms.iter().find(|p| p.id == cfg.platform).map(|p| p.dev_cost).unwrap_or(0);
        self.studio.money -= platform_cost;

        let n = size.blockers.max(1);
        let blockers = (0..size.blockers)
            .map(|i| {
                let base = (i + 1) as f32 / (n + 1) as f32;
                Blocker {
                    at: (base + self.rng.range_f32(-0.04, 0.04)).clamp(0.05, 0.95),
                    resolved: false,
                    challenge_id: None,
                }
            })
            .collect();
        let id = self.next_id();
        let name = if cfg.name.trim().is_empty() {
            self.suggest_name(content, &cfg)
        } else {
            cfg.name.trim().to_string()
        };
        let hype = (((1 + self.studio.reputation.max(0)) as f32).log10() * 8.0).min(40.0);
        self.project = Some(Project {
            id,
            name: name.clone(),
            genre: cfg.genre,
            theme: cfg.theme,
            platform: cfg.platform,
            audience: cfg.audience,
            size: cfg.size,
            focus: cfg.focus.normalized_to(100.0),
            started_week: self.date.week(),
            work_total: size.work,
            blockers,
            hype,
            budget_total: size.budget,
            cost_so_far: platform_cost,
            sequel_of: cfg.sequel_of,
            ..Project::default()
        });
        self.note(NoteKind::Info, format!("Started development of “{name}”."));
        Ok(())
    }

    /// A playful default name from genre + theme.
    pub fn suggest_name(&mut self, content: &ContentLibrary, cfg: &ProjectConfig) -> String {
        const PREFIX: [&str; 10] =
            ["Super", "Tiny", "Epic", "Neon", "Last", "Lost", "Pocket", "Midnight", "Turbo", "Rusty"];
        const SUFFIX: [&str; 10] =
            ["Quest", "Saga", "Rush", "Tycoon", "Legacy", "Odyssey", "Chronicles", "Panic", "Dash", "Deluxe"];
        let theme =
            content.themes.iter().find(|t| t.id == cfg.theme).map(|t| t.name.clone()).unwrap_or_default();
        let a = self.rng.pick(&PREFIX).copied().unwrap_or("Super");
        let b = self.rng.pick(&SUFFIX).copied().unwrap_or("Quest");
        if theme.is_empty() {
            format!("{a} {b}")
        } else {
            format!("{a} {theme} {b}")
        }
    }

    pub fn set_crunch(&mut self, on: bool) {
        if let Some(p) = self.project.as_mut() {
            p.crunch = on;
        }
    }

    /// Abandon the current project (all progress is lost).
    pub fn cancel_project(&mut self) {
        if let Some(p) = self.project.take() {
            self.note(NoteKind::Warn, format!("“{}” was cancelled.", p.name));
        }
    }

    /// Choose the next blocking challenge: the earliest unlocked topic that still has an
    /// available challenge within the studio's difficulty cap.
    pub fn pick_blocker_challenge(&mut self, content: &ContentLibrary) -> Option<String> {
        let cap = content.tier(self.studio.tier).map(|t| t.max_difficulty).unwrap_or(2);
        for topic in &content.topics {
            let candidates: Vec<&str> = content
                .challenges_in_topic(&topic.id)
                .filter(|c| {
                    c.difficulty <= cap && self.progress.availability(content, c) == Availability::Available
                })
                .map(|c| c.id.as_str())
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let i = self.rng.range_usize(0, candidates.len() - 1);
            return Some(candidates[i].to_string());
        }
        None
    }

    /// Stop development until the player solves a challenge (or nothing suitable exists).
    pub(crate) fn trigger_blocker(&mut self, content: &ContentLibrary, index: usize) {
        let challenge = self.pick_blocker_challenge(content);
        let Some(p) = self.project.as_mut() else {
            return;
        };
        match challenge.and_then(|id| content.challenge(&id)) {
            Some(c) => {
                p.blockers[index].challenge_id = Some(c.id.clone());
                let text = format!("Development of “{}” is blocked: {}", p.name, c.title);
                self.pending_attempt = Some(Attempt::new(c, ChallengeContext::Project));
                self.note(NoteKind::Warn, text);
            }
            None => {
                // Nothing left to learn at this difficulty: the team muddles through.
                p.blockers[index].resolved = true;
                p.challenge_quality += 1.5;
            }
        }
    }

    /// Called when a challenge in the `Project` context is finished.
    pub(crate) fn apply_blocker_result(&mut self, challenge_id: &str, r: &RewardSummary) {
        let Some(p) = self.project.as_mut() else {
            return;
        };
        let Some(b) =
            p.blockers.iter_mut().find(|b| b.challenge_id.as_deref() == Some(challenge_id) && !b.resolved)
        else {
            return;
        };
        b.resolved = true;
        p.work_done = (p.work_done + r.dev_points as f32 * p.work_total / 400.0).min(p.work_total);
        p.challenge_quality += r.quality;
        if r.contractor {
            p.skipped_blockers += 1;
        }
        self.progress.bump("blockers_solved", 1);
    }

    /// Release the finished game: quality, reviews, sales set-up, reputation.
    pub fn release_project(&mut self, content: &ContentLibrary) -> Result<usize, String> {
        let Some(p) = self.project.as_ref() else {
            return Err("There is nothing to release.".into());
        };
        if !p.is_complete() {
            return Err("The game is not finished yet.".into());
        }
        if self.pending_attempt.is_some() {
            return Err("Solve the blocking challenge first.".into());
        }
        let Some(p) = self.project.take() else {
            return Err("There is nothing to release.".into());
        };

        let q = quality::evaluate(content, &p, &self.games);
        let genre_name = content
            .genres
            .iter()
            .find(|g| g.id == p.genre)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| p.genre.clone());
        let (reviews, meta) = reviews::generate(content, &q, &p.focus, &p.name, &genre_name, &mut self.rng);

        let size = content.balance.size(p.size);
        let platform = content.platforms.iter().find(|pl| pl.id == p.platform);
        let price = size.price * platform.map(|pl| pl.price_mult).unwrap_or(1.0);
        let store_cut = platform.map(|pl| pl.store_cut).unwrap_or(0.25);
        let week = self.date.week();
        let demand = sales::demand(
            content,
            &DemandInput {
                genre: &p.genre,
                platform: &p.platform,
                audience: p.audience,
                size: p.size,
                hype: p.hype,
                fans: self.studio.reputation,
                metascore: meta,
                week,
                trend: 1.0,
            },
        );

        let sequel_bonus =
            p.sequel_of.and_then(|id| self.games.iter().find(|g| g.id == id)).map(|g| g.metascore);
        let size_mult = [4.0, 12.0, 40.0, 150.0][p.size.index()];
        let fans_delta = ((meta - 55.0) * size_mult).round() as i64;
        self.studio.reputation = (self.studio.reputation + fans_delta).max(0);

        let id = p.id;
        let game = ReleasedGame {
            id,
            name: p.name.clone(),
            genre: p.genre.clone(),
            theme: p.theme.clone(),
            platform: p.platform.clone(),
            audience: p.audience,
            size: p.size,
            release_week: week,
            categories: q.categories,
            quality: q.overall,
            bugs: q.bugs,
            reviews,
            metascore: meta,
            hype: p.hype,
            price,
            store_cut,
            launch_units: demand.launch_units,
            decay: demand.decay,
            dev_cost: p.cost_so_far,
            sequel_of: p.sequel_of,
            ..ReleasedGame::default()
        };
        let verdict = if meta >= 85.0 {
            NoteKind::Good
        } else if meta >= 60.0 {
            NoteKind::Info
        } else {
            NoteKind::Warn
        };
        self.note(verdict, format!("“{}” was released. Metascore {:.0}.", game.name, meta));
        let _ = sequel_bonus;
        self.games.push(game);

        self.progress.bump("games_released", 1);
        self.progress.set_max("best_metascore", meta.round() as i64);
        if meta >= 90.0 {
            self.progress.bump("games_90_plus", 1);
        }
        Ok(self.games.len() - 1)
    }

    /// Estimated current bug level of the running project (for the UI).
    pub fn project_bug_estimate(&self, content: &ContentLibrary) -> f32 {
        self.project.as_ref().map(|p| estimate_bugs(content, p)).unwrap_or(0.0)
    }
}

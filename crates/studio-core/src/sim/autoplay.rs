//! A scripted player for balance tests: it runs whole studio careers without a UI.
//!
//! The bot makes the same decisions a reasonable human would (hire when there is cash, build engine
//! modules, move into bigger offices, pick a trending genre and ship) and "solves" challenges with
//! a configurable skill instead of compiling code. Everything is deterministic for a given seed,
//! so balance regressions show up as plain test failures.

use super::challenge_flow::{Attempt, ChallengeContext};
use super::model::{Audience, ProjectSize};
use super::progress::Availability;
use super::project::ProjectConfig;
use super::rng::GameRng;
use super::state::GameState;
use crate::data::{Challenge, ContentLibrary};
use crate::settings::Difficulty;

/// How good the simulated player is at the coding challenges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotProfile {
    pub name: &'static str,
    /// Chance to solve a challenge on the first compile.
    pub first_try: f32,
    /// How many failed tries before hiring a contractor.
    pub contractor_after: u32,
    /// Self-study / engine-lab challenges the player manages per game year.
    pub study_per_year: f32,
}

impl BotProfile {
    pub const NOVICE: BotProfile =
        BotProfile { name: "novice", first_try: 0.3, contractor_after: 3, study_per_year: 6.0 };
    pub const COMPETENT: BotProfile =
        BotProfile { name: "competent", first_try: 0.6, contractor_after: 4, study_per_year: 10.0 };
    pub const EXPERT: BotProfile =
        BotProfile { name: "expert", first_try: 0.9, contractor_after: 5, study_per_year: 16.0 };
}

/// What happened during a career.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CareerReport {
    pub weeks_played: u32,
    /// `Some(week)` when the studio went bankrupt.
    pub bankrupt_at: Option<u32>,
    pub final_money: i64,
    pub peak_money: i64,
    pub lowest_money: i64,
    pub tier: usize,
    pub games_released: usize,
    pub average_meta: f32,
    pub best_meta: f32,
    pub lifetime_revenue: i64,
    pub challenges_solved: usize,
    pub contractors_hired: i64,
    pub level: u32,
    pub modules_built: usize,
    pub reputation: i64,
    /// Cash at the end of each game year.
    pub money_by_year: Vec<i64>,
    /// Studio tier at the end of each game year.
    pub tier_by_year: Vec<usize>,
    /// Week of the first release, if any.
    pub first_release_week: Option<u32>,
}

impl CareerReport {
    pub fn survived(&self) -> bool {
        self.bankrupt_at.is_none()
    }
}

/// Play `weeks` weeks of a fresh studio.
pub fn play_career(
    content: &ContentLibrary,
    seed: u64,
    difficulty: Difficulty,
    profile: &BotProfile,
    weeks: u32,
) -> CareerReport {
    play_career_with_state(content, seed, difficulty, profile, weeks).0
}

/// Like [`play_career`], but also returns the final game state for inspection.
pub fn play_career_with_state(
    content: &ContentLibrary,
    seed: u64,
    difficulty: Difficulty,
    profile: &BotProfile,
    weeks: u32,
) -> (CareerReport, GameState) {
    let mut state = GameState::new_game(content, "Bot Studio", "Bot", difficulty, Some(seed));
    let mut bot = Bot {
        rng: GameRng::from_seed(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xB07),
        profile: *profile,
        study_budget: 2.0,
    };
    let mut report = CareerReport {
        lowest_money: state.studio.money,
        peak_money: state.studio.money,
        ..Default::default()
    };

    for week in 1..=weeks {
        if state.game_over.is_some() {
            break;
        }
        bot.manage(content, &mut state);
        state.advance_week_auto(content);
        report.weeks_played = week;
        report.peak_money = report.peak_money.max(state.studio.money);
        report.lowest_money = report.lowest_money.min(state.studio.money);
        if week % 52 == 0 {
            report.money_by_year.push(state.studio.money);
            report.tier_by_year.push(state.studio.tier);
        }
        if report.first_release_week.is_none() && !state.games.is_empty() {
            report.first_release_week = Some(state.date.week());
        }
    }

    if state.game_over.is_some() {
        report.bankrupt_at = Some(state.date.week());
    }
    report.final_money = state.studio.money;
    report.tier = state.studio.tier;
    report.games_released = state.games.len();
    let metas: Vec<f32> = state.games.iter().map(|g| g.metascore).collect();
    report.average_meta = if metas.is_empty() { 0.0 } else { metas.iter().sum::<f32>() / metas.len() as f32 };
    report.best_meta = metas.iter().copied().fold(0.0, f32::max);
    report.lifetime_revenue = state.total_revenue();
    report.challenges_solved = state.progress.solved.len();
    report.contractors_hired = state.progress.stat("contractors_hired");
    report.level = state.progress.level();
    report.modules_built = state.engine.built.len();
    report.reputation = state.studio.reputation;
    (report, state)
}

struct Bot {
    rng: GameRng,
    profile: BotProfile,
    /// Study/lab solves the player can still fit in; refills a little every week.
    study_budget: f32,
}

impl Bot {
    fn manage(&mut self, content: &ContentLibrary, state: &mut GameState) {
        self.study_budget = (self.study_budget + self.profile.study_per_year / 52.0).min(4.0);
        self.solve_pending(content, state);
        self.handle_money(content, state);
        self.grow_engine(content, state);
        self.hire(content, state);
        self.upgrade(content, state);
        self.run_project(content, state);
    }

    /// Solve the blocking challenge (if any) according to the player's skill.
    fn solve_pending(&mut self, content: &ContentLibrary, state: &mut GameState) {
        let Some(attempt) = state.pending_attempt.clone() else {
            return;
        };
        let Some(challenge) = content.challenge(&attempt.challenge_id).cloned() else {
            state.pending_attempt = None;
            return;
        };
        self.solve(content, state, &challenge, attempt.context);
        state.pending_attempt = None;
    }

    /// Play one challenge to completion: failures, optional contractor, payout.
    fn solve(
        &mut self,
        content: &ContentLibrary,
        state: &mut GameState,
        challenge: &Challenge,
        context: ChallengeContext,
    ) {
        let mut attempt = Attempt::new(challenge, context);
        // Harder challenges are failed more often.
        let p = (self.profile.first_try - 0.06 * (challenge.difficulty as f32 - 2.0)).clamp(0.05, 0.98);
        let mut failures = 0;
        while !self.rng.chance(p) && failures < self.profile.contractor_after {
            failures += 1;
        }
        attempt.failed_submissions = failures;
        attempt.submissions = failures + 1;
        if failures >= self.profile.contractor_after {
            state.hire_contractor(content, challenge, &mut attempt);
        } else if failures > 0 && self.rng.chance(0.5) {
            attempt.hints_revealed = 1;
        }
        state.complete_challenge(content, challenge, &attempt);
    }

    fn handle_money(&mut self, content: &ContentLibrary, state: &mut GameState) {
        let (rent, salaries) = state.weekly_fixed_costs(content);
        let weekly = (rent + salaries).max(1);
        let outstanding = state.loan_outstanding();
        if state.studio.money < weekly * 3 {
            let want = (weekly * 8).min(state.loan_available(content));
            if want > 0 {
                let _ = state.take_loan(content, want);
            }
        } else if outstanding > 0 && state.studio.money > weekly * 20 + outstanding {
            let _ = state.repay_loan(outstanding);
        }
    }

    /// Build the cheapest module that makes sense; study the topic it needs.
    fn grow_engine(&mut self, content: &ContentLibrary, state: &mut GameState) {
        if state.engine.building.is_some() {
            return;
        }
        let (rent, salaries) = state.weekly_fixed_costs(content);
        let reserve = (rent + salaries) * 12;
        let mut modules: Vec<_> = content
            .engine_modules
            .iter()
            .filter(|m| !state.engine.has_module(&m.id) && m.min_tier <= state.studio.tier)
            .filter(|m| m.requires_modules.iter().all(|d| state.engine.has_module(d)))
            .collect();
        modules.sort_by_key(|m| m.money_cost + m.research_cost * 20);
        let Some(target) = modules.first() else {
            return;
        };
        let blockers = state.module_blockers(content, target);
        if blockers.is_empty() {
            if state.studio.money >= target.money_cost + reserve {
                let _ = state.start_module_build(content, &target.id);
            }
            return;
        }
        if self.study_budget < 1.0 {
            return;
        }
        // Solve a challenge from a topic the module still needs, else any for research points.
        let needed_topics: Vec<&str> = target
            .required_topic_solves
            .iter()
            .filter(|(t, n)| (state.progress.solved_in_topic(content, t) as u32) < *n)
            .map(|(t, _)| t.as_str())
            .collect();
        let pick = content
            .challenges
            .iter()
            .filter(|c| state.progress.availability(content, c) == Availability::Available)
            .find(|c| needed_topics.contains(&c.topic.as_str()))
            .or_else(|| {
                content
                    .challenges
                    .iter()
                    .find(|c| state.progress.availability(content, c) == Availability::Available)
            })
            .cloned();
        if let Some(c) = pick {
            self.study_budget -= 1.0;
            self.solve(content, state, &c, ChallengeContext::Engine { module: target.id.clone() });
        }
    }

    fn hire(&mut self, content: &ContentLibrary, state: &mut GameState) {
        if state.staff.len() >= state.staff_slots(content) {
            return;
        }
        if state.candidates.is_empty() {
            state.refresh_candidates(content);
        }
        let (rent, salaries) = state.weekly_fixed_costs(content);
        let weekly = rent + salaries;
        // Afford the newcomer's salary for half a year plus the recruiting fee, and keep a cushion.
        let best = state
            .candidates
            .iter()
            .filter(|c| state.studio.money > state.hire_fee(content, c) + (weekly + c.salary) * 26 + 5_000)
            .max_by(|a, b| {
                let score = |s: &super::staff::Staff| {
                    (s.programming * 1.5 + s.design + s.art + s.audio) / s.salary.max(1) as f32
                };
                score(a).total_cmp(&score(b))
            })
            .map(|c| c.id);
        if let Some(id) = best {
            let _ = state.hire(content, id);
        }
    }

    fn upgrade(&mut self, content: &ContentLibrary, state: &mut GameState) {
        let Some(status) = state.upgrade_status(content) else {
            return;
        };
        if !status.problems.is_empty() {
            return;
        }
        let (rent, salaries) = state.weekly_fixed_costs(content);
        if state.studio.money > status.cost + (rent + salaries) * 30 {
            let _ = state.upgrade_studio(content);
        }
    }

    fn run_project(&mut self, content: &ContentLibrary, state: &mut GameState) {
        if state.pending_attempt.is_some() {
            return;
        }
        if let Some(p) = state.project.as_ref() {
            if p.is_complete() && p.open_blockers() == 0 {
                let _ = state.release_project(content);
            }
            return;
        }
        let (rent, salaries) = state.weekly_fixed_costs(content);
        let cushion = (rent + salaries) * 12;
        let week = state.date.week();
        let mut best: Option<(f32, ProjectConfig)> = None;
        for size in ProjectSize::ALL.iter().rev() {
            let def = content.balance.size(*size);
            if state.studio.money < def.budget / 2 + cushion {
                continue;
            }
            for genre in &content.genres {
                for theme in &content.themes {
                    for platform in &content.platforms {
                        let cfg = ProjectConfig {
                            name: String::new(),
                            genre: genre.id.clone(),
                            theme: theme.id.clone(),
                            platform: platform.id.clone(),
                            audience: Audience::Everyone,
                            size: *size,
                            focus: genre.ideal,
                            sequel_of: None,
                        };
                        if state.project_start_problem(content, &cfg).is_some() {
                            continue;
                        }
                        let score = genre.popularity
                            * platform.market_at(week)
                            * (1.0 - platform.store_cut)
                            * platform.price_mult
                            * state.market.demand_trend(&genre.id, &theme.id)
                            * genre.theme_fit.iter().find(|(t, _)| *t == theme.id).map_or(1.0, |(_, f)| *f);
                        if best.as_ref().is_none_or(|(s, _)| score > *s) {
                            best = Some((score, cfg));
                        }
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        if let Some((_, cfg)) = best {
            let _ = state.start_project(content, cfg);
        }
    }
}

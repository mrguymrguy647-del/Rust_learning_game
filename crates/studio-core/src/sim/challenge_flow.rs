//! How a challenge attempt turns into rewards: hints, contractors and payouts.

use serde::{Deserialize, Serialize};

use super::state::GameState;
use crate::data::{Challenge, ContentLibrary};

/// Why the challenge is being played — decides what solving it pays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChallengeContext {
    /// Self-study from the Skills screen: XP only.
    Study,
    /// Replaying a solved challenge: no rewards, never touches the economy.
    Practice,
    /// Engine lab for a module: XP + research points.
    Engine { module: String },
    /// Blocking problem during game development: dev points, quality, XP.
    Project,
    /// Post-launch crisis for a released game.
    Hotfix { game_id: u32 },
    /// Game jam: timed.
    Jam { event_id: String },
}

impl ChallengeContext {
    pub fn is_practice(&self) -> bool {
        *self == ChallengeContext::Practice
    }

    pub fn label(&self) -> &'static str {
        match self {
            ChallengeContext::Study => "Study",
            ChallengeContext::Practice => "Practice",
            ChallengeContext::Engine { .. } => "Engine lab",
            ChallengeContext::Project => "Development blocker",
            ChallengeContext::Hotfix { .. } => "Hotfix",
            ChallengeContext::Jam { .. } => "Game jam",
        }
    }
}

/// State of one in-progress attempt. Stored in the save so a challenge can be resumed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub challenge_id: String,
    pub context: ChallengeContext,
    pub code: String,
    /// Compile & test submissions so far.
    pub submissions: u32,
    pub failed_submissions: u32,
    /// Number of hint tiers revealed (0..=3).
    pub hints_revealed: usize,
    pub hints_paid: i64,
    pub quiz_wrong_guesses: u32,
    pub contractor: bool,
}

impl Attempt {
    pub fn new(challenge: &Challenge, context: ChallengeContext) -> Attempt {
        Attempt {
            challenge_id: challenge.id.clone(),
            context,
            code: challenge.starter_code.clone(),
            submissions: 0,
            failed_submissions: 0,
            hints_revealed: 0,
            hints_paid: 0,
            quiz_wrong_guesses: 0,
            contractor: false,
        }
    }

    pub fn failures(&self) -> u32 {
        self.failed_submissions + self.quiz_wrong_guesses
    }

    /// May the player hire a contractor? (Never blocks progress permanently.)
    pub fn contractor_available(&self, after_failures: u32) -> bool {
        self.failures() >= after_failures
    }
}

/// How the player pays for a hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintPayment {
    Money,
    /// Spend a week researching: the clock advances (rent and salaries still run).
    Time,
}

/// Payout multiplier from how cleanly the challenge was solved.
pub fn quality_multiplier(first_try: bool, hints_used: usize, contractor: bool) -> f32 {
    if contractor {
        return 0.25;
    }
    let hint_penalty = (1.0 - 0.12 * hints_used as f32).max(0.5);
    let first_try_bonus = if first_try { 1.5 } else { 1.0 };
    hint_penalty * first_try_bonus
}

/// Everything the UI needs to celebrate (or explain) a solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RewardSummary {
    pub xp: u32,
    pub research: u32,
    pub dev_points: u32,
    pub quality: f32,
    pub first_time: bool,
    pub first_try: bool,
    pub contractor: bool,
    pub level_up: Option<u32>,
    pub new_achievements: Vec<String>,
    pub new_codex: Vec<String>,
}

impl GameState {
    /// Cost in dollars of the next hint tier, already scaled by `multiplier`.
    pub fn hint_cost(&self, challenge: &Challenge, tier: usize, multiplier: f32, attempt: &Attempt) -> i64 {
        if attempt.context.is_practice() {
            return 0;
        }
        challenge.hint_cost(tier, multiplier)
    }

    /// Reveal the next hint, paying with money or time. Returns the hint text, or `None`
    /// if there is nothing left to reveal or the studio cannot afford it.
    pub fn buy_hint(
        &mut self,
        content: &ContentLibrary,
        challenge: &Challenge,
        attempt: &mut Attempt,
        payment: HintPayment,
        multiplier: f32,
    ) -> Option<String> {
        let tier = attempt.hints_revealed;
        let text = challenge.hints.get(tier)?.clone();
        let cost = self.hint_cost(challenge, tier, multiplier, attempt);
        match payment {
            _ if cost == 0 => {}
            HintPayment::Money => {
                if self.studio.money < cost {
                    return None;
                }
                self.studio.money -= cost;
                attempt.hints_paid += cost;
            }
            HintPayment::Time => {
                self.advance_weeks_quiet(content, 1);
            }
        }
        attempt.hints_revealed += 1;
        if !attempt.context.is_practice() {
            self.progress.bump("hints_used", 1);
        }
        Some(text)
    }

    /// Hire a contractor: expensive, but the solution is still shown. Money may go negative —
    /// progress is never blocked.
    pub fn hire_contractor(&mut self, challenge: &Challenge, attempt: &mut Attempt, multiplier: f32) -> i64 {
        let cost = if attempt.context.is_practice() { 0 } else { challenge.contractor_cost(multiplier) };
        self.studio.money -= cost;
        attempt.contractor = true;
        if !attempt.context.is_practice() {
            self.progress.bump("contractors_hired", 1);
        }
        cost
    }

    /// Record a finished challenge and pay out rewards for its context.
    pub fn complete_challenge(
        &mut self,
        content: &ContentLibrary,
        challenge: &Challenge,
        attempt: &Attempt,
    ) -> RewardSummary {
        let mut summary = RewardSummary {
            first_try: attempt.failures() == 0 && attempt.hints_revealed == 0 && !attempt.contractor,
            contractor: attempt.contractor,
            ..RewardSummary::default()
        };
        if attempt.context.is_practice() {
            // Practice never changes progress, statistics or the economy.
            return summary;
        }

        let already_solved = self.progress.is_solved(&challenge.id);
        summary.first_time = !already_solved;
        let base = challenge.effective_rewards();
        let mult = quality_multiplier(summary.first_try, attempt.hints_revealed, attempt.contractor);
        // Re-solving something already learned pays only a token amount.
        let repeat = if already_solved { 0.1 } else { 1.0 };

        summary.xp = (base.xp as f32 * mult * repeat).round() as u32;
        match &attempt.context {
            ChallengeContext::Study | ChallengeContext::Practice => {}
            ChallengeContext::Engine { .. } => {
                summary.research = (base.research as f32 * mult.max(0.5) * repeat).round() as u32;
            }
            ChallengeContext::Project | ChallengeContext::Hotfix { .. } | ChallengeContext::Jam { .. } => {
                summary.dev_points =
                    (base.dev_points as f32 * if attempt.contractor { 0.6 } else { 1.0 }) as u32;
                summary.quality = base.quality * mult * repeat.max(0.5);
            }
        }

        let level_before = self.progress.level();
        self.progress.xp += summary.xp;
        self.studio.research_points += summary.research as i64;

        let rec = self.progress.solved.entry(challenge.id.clone()).or_default();
        rec.attempts = attempt.submissions.max(1);
        rec.first_try = summary.first_try;
        rec.hints_used = attempt.hints_revealed as u32;
        // A contractor never overwrites a genuine earlier solve.
        rec.contractor = attempt.contractor && !(already_solved && !rec.contractor);
        rec.solved_week = self.date.week();
        rec.times_solved += 1;

        if summary.first_time {
            self.progress.bump("challenges_solved", 1);
            if summary.first_try {
                self.progress.bump("first_try_solves", 1);
            }
            let solved_now = self.progress.solved.len() as i64;
            self.progress.set_max("unique_challenges", solved_now);
            let topic_done =
                content.challenges_in_topic(&challenge.topic).all(|c| self.progress.is_solved(&c.id));
            if topic_done {
                self.progress.bump("topics_completed", 1);
            }
            for e in content.codex.iter().filter(|e| e.unlock_challenge.as_deref() == Some(&challenge.id)) {
                summary.new_codex.push(e.id.clone());
            }
        }

        if matches!(attempt.context, ChallengeContext::Project) {
            self.apply_blocker_result(&challenge.id, &summary);
        }

        for a in self.progress.check_achievements(&content.achievements) {
            summary.new_achievements.push(a.name.clone());
        }
        let level_after = self.progress.level();
        if level_after > level_before {
            summary.level_up = Some(level_after);
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;

    fn setup() -> (ContentLibrary, GameState) {
        (
            ContentLibrary::embedded(),
            GameState::new_game(&ContentLibrary::embedded(), "T", "P", Difficulty::Normal, Some(1)),
        )
    }

    fn first_code_challenge(lib: &ContentLibrary) -> &Challenge {
        lib.challenges.iter().find(|c| !c.is_quiz()).expect("content has a code challenge")
    }

    #[test]
    fn clean_first_try_pays_more_than_hinted() {
        assert!(quality_multiplier(true, 0, false) > quality_multiplier(false, 0, false));
        assert!(quality_multiplier(false, 0, false) > quality_multiplier(false, 3, false));
        assert!(quality_multiplier(false, 3, false) >= 0.5 * 0.6, "hint penalty is bounded");
        assert!(quality_multiplier(true, 0, true) < quality_multiplier(false, 3, false));
    }

    #[test]
    fn study_solve_pays_xp_only_and_is_recorded() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let attempt = Attempt::new(c, ChallengeContext::Study);
        let money = st.studio.money;
        let r = st.complete_challenge(&lib, c, &attempt);
        assert!(r.first_try && r.first_time);
        assert!(r.xp > 0);
        assert_eq!((r.research, r.dev_points), (0, 0));
        assert_eq!(st.studio.money, money);
        assert!(st.progress.is_solved(&c.id));
        assert_eq!(st.progress.stat("challenges_solved"), 1);
    }

    #[test]
    fn engine_context_pays_research_points() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let attempt = Attempt::new(c, ChallengeContext::Engine { module: "core_loop".into() });
        let r = st.complete_challenge(&lib, c, &attempt);
        assert!(r.research > 0);
        assert_eq!(st.studio.research_points, r.research as i64);
    }

    #[test]
    fn resolving_pays_only_a_token_amount() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let a = Attempt::new(c, ChallengeContext::Study);
        let first = st.complete_challenge(&lib, c, &a);
        let again = st.complete_challenge(&lib, c, &a);
        assert!(!again.first_time);
        assert!(again.xp * 5 < first.xp);
        assert_eq!(st.progress.stat("challenges_solved"), 1);
    }

    #[test]
    fn practice_never_pays_or_records() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let a = Attempt::new(c, ChallengeContext::Practice);
        let before = st.clone();
        let r = st.complete_challenge(&lib, c, &a);
        assert_eq!((r.xp, r.research, r.dev_points), (0, 0, 0));
        assert_eq!(st.progress.xp, before.progress.xp);
        assert!(!st.progress.is_solved(&c.id));
        assert_eq!(st.studio.money, before.studio.money);
    }

    #[test]
    fn hints_cost_money_in_order_and_stop_when_exhausted() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let mut a = Attempt::new(c, ChallengeContext::Study);
        let start = st.studio.money;
        let h1 = st.buy_hint(&lib, c, &mut a, HintPayment::Money, 1.0).unwrap();
        assert_eq!(h1, c.hints[0]);
        assert_eq!(st.studio.money, start - c.hint_cost(0, 1.0));
        st.buy_hint(&lib, c, &mut a, HintPayment::Money, 1.0).unwrap();
        st.buy_hint(&lib, c, &mut a, HintPayment::Money, 1.0).unwrap();
        assert!(st.buy_hint(&lib, c, &mut a, HintPayment::Money, 1.0).is_none());
        assert_eq!(a.hints_revealed, 3);
        assert_eq!(a.hints_paid, start - st.studio.money);
        assert!(c.hint_cost(2, 1.0) > c.hint_cost(0, 1.0));
    }

    #[test]
    fn unaffordable_hint_is_refused_and_free_hints_work() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        st.studio.money = 0;
        let mut a = Attempt::new(c, ChallengeContext::Study);
        assert!(st.buy_hint(&lib, c, &mut a, HintPayment::Money, 1.0).is_none());
        assert_eq!(a.hints_revealed, 0);
        assert!(st.buy_hint(&lib, c, &mut a, HintPayment::Money, 0.0).is_some(), "multiplier 0 = free hints");
    }

    #[test]
    fn time_payment_advances_the_calendar_instead_of_money() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let mut a = Attempt::new(c, ChallengeContext::Study);
        let (money, week) = (st.studio.money, st.date.week());
        st.buy_hint(&lib, c, &mut a, HintPayment::Time, 1.0).unwrap();
        assert_eq!(st.date.week(), week + 1);
        assert!(st.studio.money <= money);
    }

    #[test]
    fn contractor_is_expensive_marks_attempt_and_discounts_rewards() {
        let (lib, mut st) = setup();
        let c = first_code_challenge(&lib);
        let mut a = Attempt::new(c, ChallengeContext::Study);
        st.studio.money = 10;
        let cost = st.hire_contractor(c, &mut a, 1.0);
        assert_eq!(st.studio.money, 10 - cost, "money may go negative; progress is never blocked");
        assert!(a.contractor);
        let r = st.complete_challenge(&lib, c, &a);
        assert!(r.contractor && !r.first_try);
        assert!(st.progress.solved[&c.id].contractor);
    }

    #[test]
    fn contractor_is_only_offered_after_enough_failures() {
        let (lib, _) = setup();
        let c = first_code_challenge(&lib);
        let mut a = Attempt::new(c, ChallengeContext::Study);
        assert!(!a.contractor_available(3));
        a.failed_submissions = 2;
        a.quiz_wrong_guesses = 1;
        assert!(a.contractor_available(3));
    }
}

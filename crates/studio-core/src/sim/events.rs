//! Random events: decisions, crises with hotfix challenges, game jams, publisher deals.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::challenge_flow::{Attempt, ChallengeContext};
use super::model::ProjectSize;
use super::notify::NoteKind;
use super::state::GameState;
use crate::data::{ChallengeKind, ChoiceAction, Cond, ContentLibrary, Effect, EventDef, EventKind};

/// The event waiting for the player's decision (the clock is paused).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveEvent {
    pub def_id: String,
    pub week: u32,
    /// The released game the event is about, if any.
    pub game_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub week: u32,
    pub event_id: String,
    pub game_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventLog {
    pub week: u32,
    pub title: String,
    pub outcome: String,
}

/// An accepted publisher contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    pub publisher: String,
    /// Required genre; empty = any.
    pub genre: String,
    pub min_size: ProjectSize,
    pub deadline_week: u32,
    pub min_meta: f32,
    pub advance: i64,
    pub bonus: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EventState {
    pub pending: Option<ActiveEvent>,
    /// Event id -> first week it may appear again.
    pub cooldowns: BTreeMap<String, u32>,
    pub scheduled: Vec<ScheduledEvent>,
    pub log: Vec<EventLog>,
    pub contract: Option<Contract>,
}

/// What happened after the player chose.
#[derive(Debug, Clone, PartialEq)]
pub enum EventOutcome {
    Done(String),
    /// A challenge was started (stored in `pending_attempt`).
    Challenge(String),
}

/// Weekly chance that a random event shows up.
const EVENT_CHANCE: f32 = 0.07;
const MAX_LOG: usize = 40;

impl GameState {
    /// Text with `{game}` / `{studio}` filled in.
    pub fn event_text(&self, text: &str, game_id: Option<u32>) -> String {
        let game = game_id
            .and_then(|id| self.games.iter().find(|g| g.id == id))
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "your game".into());
        let filled = text.replace("{game}", &game).replace("{studio}", &self.studio.name);
        // A placeholder at the start of a sentence ("your game crashes…") still needs a capital.
        let mut chars = filled.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => filled,
        }
    }

    fn cond_met(&self, c: &Cond) -> bool {
        match c {
            Cond::HasProject => self.project.is_some(),
            Cond::NoProject => self.project.is_none(),
            Cond::HasReleasedGame => self.games.iter().any(|g| !g.is_dlc),
            Cond::NoContract => self.events.contract.is_none(),
            Cond::MinMoney(m) => self.studio.money >= *m,
            Cond::MinFans(f) => self.studio.reputation >= *f,
            Cond::MinStaff(n) => self.staff.len() as u32 >= *n,
        }
    }

    /// A released game a crisis can hit: recent, still selling, preferably buggy.
    fn hotfix_target(&self) -> Option<u32> {
        let week = self.date.week();
        self.games
            .iter()
            .filter(|g| !g.is_dlc && g.on_sale && g.age(week) <= 40)
            .max_by(|a, b| a.bugs.total_cmp(&b.bugs))
            .map(|g| g.id)
    }

    fn event_weight(&self, content: &ContentLibrary, def: &EventDef) -> f32 {
        let week = self.date.week();
        if self.studio.tier < def.min_tier || self.studio.tier > def.max_tier || week < def.min_week {
            return 0.0;
        }
        if self.events.cooldowns.get(&def.id).is_some_and(|until| week < *until) {
            return 0.0;
        }
        if !def.requires.iter().all(|c| self.cond_met(c)) {
            return 0.0;
        }
        let _ = content;
        match def.kind {
            EventKind::Hotfix => {
                match self.hotfix_target().and_then(|id| self.games.iter().find(|g| g.id == id)) {
                    Some(g) => def.weight * (0.4 + g.bugs / 15.0),
                    None => 0.0,
                }
            }
            _ => def.weight,
        }
    }

    fn trigger_event(&mut self, def: &EventDef, game_id: Option<u32>) {
        let week = self.date.week();
        self.events.cooldowns.insert(def.id.clone(), week + def.cooldown_weeks);
        self.events.pending = Some(ActiveEvent { def_id: def.id.clone(), week, game_id });
        self.note(NoteKind::Info, format!("Event: {}", def.title));
    }

    /// Make an event happen right now (developer tools and tests).
    pub fn force_event(&mut self, content: &ContentLibrary, event_id: &str, game_id: Option<u32>) -> bool {
        let Some(def) = content.event(event_id).cloned() else {
            return false;
        };
        self.trigger_event(&def, game_id);
        true
    }

    /// Weekly: scheduled events first, then a small random chance.
    pub(crate) fn maybe_trigger_event(&mut self, content: &ContentLibrary) {
        if self.events.pending.is_some() || self.pending_attempt.is_some() {
            return;
        }
        let week = self.date.week();
        if let Some(pos) = self.events.scheduled.iter().position(|s| s.week <= week) {
            let s = self.events.scheduled.remove(pos);
            if let Some(def) = content.event(&s.event_id).cloned() {
                self.trigger_event(&def, s.game_id);
                return;
            }
        }
        if !self.rng.chance(EVENT_CHANCE) {
            return;
        }
        let weights: Vec<f32> = content.events.iter().map(|d| self.event_weight(content, d)).collect();
        let Some(i) = self.rng.weighted_index(&weights) else {
            return;
        };
        let def = content.events[i].clone();
        let game_id = match def.kind {
            EventKind::Hotfix => self.hotfix_target(),
            _ => {
                let recent: Vec<u32> =
                    self.games.iter().rev().filter(|g| !g.is_dlc).take(3).map(|g| g.id).collect();
                self.rng.pick(&recent).copied()
            }
        };
        self.trigger_event(&def, game_id);
    }

    /// Schedule a crisis for a freshly released, buggy game.
    pub(crate) fn schedule_launch_crisis(&mut self, content: &ContentLibrary, game_id: u32) {
        let Some(g) = self.games.iter().find(|g| g.id == game_id) else {
            return;
        };
        let p = ((g.bugs - 8.0) / 25.0).clamp(0.0, 0.85);
        if !self.rng.chance(p) {
            return;
        }
        let hotfixes: Vec<&EventDef> =
            content.events.iter().filter(|e| e.kind == EventKind::Hotfix).collect();
        if let Some(def) = self.rng.pick(&hotfixes) {
            let week = self.date.week() + self.rng.range_i64(1, 3) as u32;
            self.events.scheduled.push(ScheduledEvent {
                week,
                event_id: def.id.clone(),
                game_id: Some(game_id),
            });
        }
    }

    fn apply_effect(&mut self, effect: &Effect, game_id: Option<u32>) {
        match effect {
            Effect::Money(m) => self.studio.money += *m,
            Effect::Fans(f) => self.studio.reputation = (self.studio.reputation + *f).max(0),
            Effect::Morale(m) => {
                for s in self.staff.iter_mut() {
                    s.morale = (s.morale + *m).clamp(0.0, 100.0);
                }
            }
            Effect::Hype(h) => {
                if let Some(p) = self.project.as_mut() {
                    p.hype = (p.hype + *h).clamp(0.0, 100.0);
                }
            }
            Effect::Work(f) => {
                if let Some(p) = self.project.as_mut() {
                    p.work_done = (p.work_done + f * p.work_total).clamp(0.0, p.work_total);
                }
            }
            Effect::CrunchBugs(b) => {
                if let Some(p) = self.project.as_mut() {
                    p.crunch_bugs += *b;
                }
            }
            Effect::Research(r) => self.studio.research_points += *r,
            Effect::Xp(x) => self.progress.xp += *x,
            Effect::TargetBugs(b) => {
                if let Some(g) = game_id.and_then(|id| self.games.iter_mut().find(|g| g.id == id)) {
                    g.bugs = (g.bugs + *b).clamp(0.0, 100.0);
                }
            }
            Effect::TargetSalesMult(m) => {
                if let Some(g) = game_id.and_then(|id| self.games.iter_mut().find(|g| g.id == id)) {
                    g.launch_units *= *m;
                }
            }
        }
    }

    /// Pick a challenge for a hotfix or jam: unsolved and available when possible.
    fn pick_event_challenge(&mut self, content: &ContentLibrary, timed: bool) -> Option<String> {
        let cap = content.tier(self.studio.tier).map(|t| t.max_difficulty).unwrap_or(2);
        let max_d = if timed { cap.min(3) } else { cap };
        let quick = |k: ChallengeKind| {
            !timed
                || matches!(
                    k,
                    ChallengeKind::FixCompile
                        | ChallengeKind::Complete
                        | ChallengeKind::FindBug
                        | ChallengeKind::Refactor
                        | ChallengeKind::PredictOutput
                        | ChallengeKind::CodeReview
                )
        };
        // Hotfixes are bugs: prefer bug-hunting kinds.
        let bug_kind = |k: ChallengeKind| {
            timed || matches!(k, ChallengeKind::FixCompile | ChallengeKind::FindBug | ChallengeKind::Complete)
        };
        let fresh: Vec<&str> = content
            .challenges
            .iter()
            .filter(|c| {
                c.difficulty <= max_d
                    && quick(c.kind)
                    && bug_kind(c.kind)
                    && self.progress.availability(content, c) == super::progress::Availability::Available
            })
            .map(|c| c.id.as_str())
            .collect();
        if let Some(id) = self.rng.pick(&fresh) {
            return Some((*id).to_string());
        }
        // Everything suitable is solved: replay a solved one (reduced rewards).
        let replay: Vec<&str> = content
            .challenges
            .iter()
            .filter(|c| c.difficulty <= max_d && quick(c.kind) && self.progress.is_solved(&c.id))
            .map(|c| c.id.as_str())
            .collect();
        self.rng.pick(&replay).map(|s| (*s).to_string())
    }

    /// Apply the player's choice. On `Err` nothing changed and the event stays open.
    pub fn resolve_event(&mut self, content: &ContentLibrary, choice: usize) -> Result<EventOutcome, String> {
        let Some(active) = self.events.pending.clone() else {
            return Err("There is no event to resolve.".into());
        };
        let def = content.event(&active.def_id).cloned().ok_or("Unknown event.")?;
        let c = def.choices.get(choice).cloned().ok_or("Unknown choice.")?;
        if self.studio.money < c.cost {
            return Err(format!("That costs {}.", crate::fmt::money(c.cost)));
        }
        self.studio.money -= c.cost;
        for e in &c.effects {
            self.apply_effect(e, active.game_id);
        }
        self.events.pending = None;

        let mut outcome = EventOutcome::Done(self.event_text(&c.label, active.game_id));
        match c.action {
            ChoiceAction::None => {}
            ChoiceAction::AcceptContract => {
                if let Some(t) = &def.contract {
                    let week = self.date.week();
                    self.studio.money += t.advance;
                    self.events.contract = Some(Contract {
                        publisher: t.publisher.clone(),
                        genre: t.genre.clone(),
                        min_size: t.min_size,
                        deadline_week: week + t.weeks,
                        min_meta: t.min_meta,
                        advance: t.advance,
                        bonus: t.bonus,
                    });
                    self.note(
                        NoteKind::Good,
                        format!(
                            "Contract signed with {}: {} advance received.",
                            t.publisher,
                            crate::fmt::money(t.advance)
                        ),
                    );
                }
            }
            ChoiceAction::StartHotfix => {
                match self.pick_event_challenge(content, false).and_then(|id| content.challenge(&id)) {
                    Some(ch) => {
                        let game_id = active.game_id.unwrap_or(0);
                        self.pending_attempt = Some(Attempt::new(ch, ChallengeContext::Hotfix { game_id }));
                        outcome = EventOutcome::Challenge(ch.id.clone());
                    }
                    None => {
                        // No suitable challenge in the catalogue: the team patches it the old-fashioned way.
                        self.apply_effect(&Effect::TargetBugs(-4.0), active.game_id);
                        outcome = EventOutcome::Done("The team patched it by hand.".into());
                    }
                }
            }
            ChoiceAction::StartJam => {
                match self.pick_event_challenge(content, true).and_then(|id| content.challenge(&id)) {
                    Some(ch) => {
                        let mut attempt =
                            Attempt::new(ch, ChallengeContext::Jam { event_id: def.id.clone() });
                        attempt.time_limit_secs = Some(def.jam_minutes.max(1) * 60);
                        self.pending_attempt = Some(attempt);
                        outcome = EventOutcome::Challenge(ch.id.clone());
                    }
                    None => outcome = EventOutcome::Done("No jam challenge was available.".into()),
                }
            }
        }
        let text = match &outcome {
            EventOutcome::Done(t) => t.clone(),
            EventOutcome::Challenge(_) => c.label.clone(),
        };
        self.events.log.push(EventLog { week: self.date.week(), title: def.title.clone(), outcome: text });
        if self.events.log.len() > MAX_LOG {
            self.events.log.remove(0);
        }
        Ok(outcome)
    }

    /// Resolve a pending event without opening any challenge: the first affordable choice with no
    /// special action, else the last choice. Used by bots, tests and developer tools.
    pub fn auto_resolve_event(&mut self, content: &ContentLibrary) -> bool {
        let Some(active) = self.events.pending.clone() else {
            return false;
        };
        let Some(def) = content.event(&active.def_id) else {
            self.events.pending = None;
            return true;
        };
        let pick = def
            .choices
            .iter()
            .position(|c| c.action == ChoiceAction::None && c.cost <= self.studio.money)
            .unwrap_or(def.choices.len().saturating_sub(1));
        if self.resolve_event(content, pick).is_err() {
            self.events.pending = None;
        }
        // A challenge (hotfix/jam) cannot be auto-solved: abandon it.
        if self.pending_attempt.as_ref().is_some_and(|a| {
            matches!(a.context, ChallengeContext::Hotfix { .. } | ChallengeContext::Jam { .. })
        }) {
            self.pending_attempt = None;
        }
        true
    }

    /// `advance_week` that first settles any pending event (for bots and tests).
    pub fn advance_week_auto(&mut self, content: &ContentLibrary) {
        self.auto_resolve_event(content);
        self.advance_week(content);
    }

    // ---- contracts -------------------------------------------------------------------------

    /// Called when a game is released: pays or punishes the publisher contract if it applies.
    pub(crate) fn settle_contract(&mut self, genre: &str, size: ProjectSize, meta: f32) {
        let Some(c) = self.events.contract.clone() else {
            return;
        };
        if (!c.genre.is_empty() && c.genre != genre) || size < c.min_size {
            return;
        }
        let week = self.date.week();
        if week <= c.deadline_week && meta >= c.min_meta {
            self.studio.money += c.bonus;
            self.studio.reputation += 150;
            self.progress.bump("contracts_fulfilled", 1);
            self.note(
                NoteKind::Good,
                format!("{} is delighted: bonus of {} paid.", c.publisher, crate::fmt::money(c.bonus)),
            );
        } else {
            let penalty = c.advance / 2;
            self.studio.money -= penalty;
            self.note(
                NoteKind::Bad,
                format!(
                    "{} is unhappy with the delivery (late or too weak). You repay {}.",
                    c.publisher,
                    crate::fmt::money(penalty)
                ),
            );
        }
        self.events.contract = None;
    }

    /// Weekly: a contract whose deadline passed without delivery is cancelled.
    pub(crate) fn check_contract_deadline(&mut self) {
        let Some(c) = self.events.contract.clone() else {
            return;
        };
        if self.date.week() > c.deadline_week {
            let penalty = c.advance / 2;
            self.studio.money -= penalty;
            self.note(
                NoteKind::Bad,
                format!(
                    "You missed the deadline for {}. You repay {}.",
                    c.publisher,
                    crate::fmt::money(penalty)
                ),
            );
            self.events.contract = None;
        }
    }

    // ---- results of event challenges ------------------------------------------------------------

    pub(crate) fn apply_hotfix_result(&mut self, game_id: u32, contractor: bool, repeat: bool) {
        let Some(g) = self.games.iter_mut().find(|g| g.id == game_id) else {
            return;
        };
        let cut = if contractor {
            4.0
        } else if repeat {
            6.0
        } else {
            10.0
        };
        g.bugs = (g.bugs - cut).max(0.0);
        let name = g.name.clone();
        g.boost_weeks = g.boost_weeks.max(2);
        g.boost_mult = g.boost_mult.max(1.15);
        self.studio.reputation += if contractor { 5 } else { 40 };
        self.progress.bump("hotfixes_done", 1);
        self.note(NoteKind::Good, format!("The hotfix for “{name}” is live. Players are happy again."));
    }

    pub(crate) fn apply_jam_result(
        &mut self,
        content: &ContentLibrary,
        event_id: &str,
        attempt: &Attempt,
        repeat: bool,
    ) {
        let Some(def) = content.event(event_id) else {
            return;
        };
        let limit = attempt.time_limit_secs.unwrap_or(1).max(1) as f32;
        let speed = (1.0 - attempt.elapsed_secs / limit).clamp(0.0, 1.0);
        let mut factor = 0.5 + 0.5 * speed;
        if attempt.contractor {
            factor *= 0.2;
        }
        if repeat {
            factor *= 0.5;
        }
        let prize = (def.jam_prize as f32 * factor).round() as i64;
        let fans = (def.jam_fans as f32 * factor).round() as i64;
        self.studio.money += prize;
        self.studio.reputation += fans;
        self.progress.bump("jams_won", 1);
        self.note(
            NoteKind::Good,
            format!("{}: prize {} and {fans} new fans!", def.title, crate::fmt::money(prize)),
        );
    }

    /// The jam clock ran out.
    pub fn fail_jam(&mut self, content: &ContentLibrary, event_id: &str) {
        let title = content.event(event_id).map(|d| d.title.clone()).unwrap_or_else(|| "The jam".into());
        self.pending_attempt = None;
        self.note(NoteKind::Warn, format!("{title}: time ran out. Better luck next year!"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        let mut st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(8));
        st.studio.money = 1_000_000;
        (lib, st)
    }

    fn trigger(lib: &ContentLibrary, st: &mut GameState, id: &str, game: Option<u32>) {
        let def = lib.event(id).unwrap().clone();
        st.trigger_event(&def, game);
    }

    fn add_game(st: &mut GameState, id: u32, bugs: f32) {
        st.games.push(crate::sim::ReleasedGame {
            id,
            name: format!("Game{id}"),
            bugs,
            release_week: st.date.week(),
            launch_units: 1000.0,
            ..Default::default()
        });
    }

    #[test]
    fn every_event_in_the_content_is_well_formed() {
        let lib = ContentLibrary::embedded();
        assert!(lib.events.len() >= 20);
        for e in &lib.events {
            assert!(!e.choices.is_empty(), "{}", e.id);
            assert!(e.choices.iter().all(|c| !c.label.is_empty()), "{}", e.id);
            if e.kind == EventKind::Publisher {
                assert!(
                    e.contract.is_some()
                        && e.choices.iter().any(|c| c.action == ChoiceAction::AcceptContract),
                    "{}",
                    e.id
                );
                assert!(e.requires.contains(&Cond::NoContract));
            }
            if e.kind == EventKind::GameJam {
                assert!(
                    e.jam_minutes > 0
                        && e.jam_prize > 0
                        && e.choices.iter().any(|c| c.action == ChoiceAction::StartJam),
                    "{}",
                    e.id
                );
            }
            if e.kind == EventKind::Hotfix {
                assert!(e.choices.iter().any(|c| c.action == ChoiceAction::StartHotfix), "{}", e.id);
                assert!(e.requires.contains(&Cond::HasReleasedGame));
            }
            if let Some(c) = &e.contract {
                if !c.genre.is_empty() {
                    assert!(lib.genre(&c.genre).is_some(), "{}: unknown genre", e.id);
                }
            }
        }
    }

    #[test]
    fn random_events_eventually_happen_and_pause_the_clock() {
        let (lib, mut st) = setup();
        let mut seen = false;
        for _ in 0..400 {
            st.advance_week(&lib);
            if st.events.pending.is_some() {
                seen = true;
                break;
            }
        }
        assert!(seen, "an event appears within 400 weeks");
        let week = st.date.week();
        st.advance_week(&lib);
        assert_eq!(st.date.week(), week, "the clock waits for the decision");
    }

    #[test]
    fn choices_apply_effects_and_costs() {
        let (lib, mut st) = setup();
        trigger(&lib, &mut st, "coffee_machine", None);
        let money = st.studio.money;
        st.staff[0].morale = 50.0;
        let out = st.resolve_event(&lib, 0).unwrap();
        assert!(matches!(out, EventOutcome::Done(_)));
        assert_eq!(st.studio.money, money - 400);
        assert_eq!(st.staff[0].morale, 58.0);
        assert!(st.events.pending.is_none());
        assert_eq!(st.events.log.len(), 1);
    }

    #[test]
    fn unaffordable_choices_are_refused_and_keep_the_event_open() {
        let (lib, mut st) = setup();
        st.studio.money = 100;
        trigger(&lib, &mut st, "coffee_machine", None);
        assert!(st.resolve_event(&lib, 0).unwrap_err().contains("costs"));
        assert!(st.events.pending.is_some());
        assert!(st.resolve_event(&lib, 1).is_ok(), "the free option still works");
    }

    #[test]
    fn cooldowns_prevent_immediate_repeats() {
        let (lib, mut st) = setup();
        trigger(&lib, &mut st, "coffee_machine", None);
        st.resolve_event(&lib, 1).unwrap();
        let def = lib.event("coffee_machine").unwrap();
        assert_eq!(st.event_weight(&lib, def), 0.0);
        st.date.advance(def.cooldown_weeks + 1);
        assert!(st.event_weight(&lib, def) > 0.0);
    }

    #[test]
    fn hotfix_starts_a_blocking_challenge_that_reduces_bugs_when_solved() {
        let (lib, mut st) = setup();
        add_game(&mut st, 7, 30.0);
        trigger(&lib, &mut st, "hotfix_crash", Some(7));
        let out = st.resolve_event(&lib, 0).unwrap();
        let EventOutcome::Challenge(id) = out else { panic!("expected a challenge") };
        let attempt = st.pending_attempt.clone().unwrap();
        assert_eq!(attempt.context, ChallengeContext::Hotfix { game_id: 7 });
        let c = lib.challenge(&id).unwrap().clone();
        let mut done = attempt.clone();
        done.submissions = 1;
        st.complete_challenge(&lib, &c, &done);
        assert!(st.games[0].bugs < 25.0, "{}", st.games[0].bugs);
        assert!(st.progress.stat("hotfixes_done") >= 1);
    }

    #[test]
    fn ignoring_a_crisis_hurts() {
        let (lib, mut st) = setup();
        add_game(&mut st, 7, 30.0);
        st.studio.reputation = 500;
        trigger(&lib, &mut st, "hotfix_crash", Some(7));
        st.resolve_event(&lib, 2).unwrap();
        assert!(st.games[0].launch_units < 1000.0);
        assert_eq!(st.studio.reputation, 460);
    }

    #[test]
    fn game_jam_is_timed_and_pays_more_for_speed() {
        let (lib, mut st) = setup();
        trigger(&lib, &mut st, "jam_rusty", None);
        let EventOutcome::Challenge(id) = st.resolve_event(&lib, 0).unwrap() else {
            panic!("expected a challenge")
        };
        let attempt = st.pending_attempt.clone().unwrap();
        assert_eq!(attempt.time_limit_secs, Some(12 * 60));
        let c = lib.challenge(&id).unwrap();
        assert!(c.difficulty <= 3 && !c.is_quiz() || c.is_quiz());

        let payout = |elapsed: f32| {
            let mut s = st.clone();
            s.pending_attempt = None;
            let mut a = attempt.clone();
            a.elapsed_secs = elapsed;
            let before = s.studio.money;
            s.apply_jam_result(&lib, "jam_rusty", &a, false);
            s.studio.money - before
        };
        assert!(payout(30.0) > payout(600.0));
        assert!(payout(600.0) >= 1250);
        let mut failed = st.clone();
        failed.fail_jam(&lib, "jam_rusty");
        assert!(failed.pending_attempt.is_none());
    }

    #[test]
    fn publisher_contract_pays_a_bonus_only_when_delivered_in_time_with_enough_quality() {
        let (lib, mut st) = setup();
        trigger(&lib, &mut st, "pub_quirky", None);
        let before = st.studio.money;
        st.resolve_event(&lib, 0).unwrap();
        assert_eq!(st.studio.money, before + 6000);
        assert!(st.events.contract.is_some());

        let mut good = st.clone();
        let m = good.studio.money;
        good.settle_contract("puzzle", ProjectSize::Small, 72.0);
        assert_eq!(good.studio.money, m + 12000);
        assert!(good.events.contract.is_none());

        let mut weak = st.clone();
        let m = weak.studio.money;
        weak.settle_contract("puzzle", ProjectSize::Small, 40.0);
        assert_eq!(weak.studio.money, m - 3000);

        let mut other_genre = st.clone();
        other_genre.settle_contract("racing", ProjectSize::Small, 90.0);
        assert!(other_genre.events.contract.is_some(), "an unrelated game does not count");

        let mut late = st.clone();
        late.date.advance(31);
        let m = late.studio.money;
        late.check_contract_deadline();
        assert_eq!(late.studio.money, m - 3000);
        assert!(late.events.contract.is_none());
    }

    #[test]
    fn buggy_launches_schedule_hotfix_crises() {
        let (lib, mut st) = setup();
        add_game(&mut st, 3, 60.0);
        for _ in 0..30 {
            st.schedule_launch_crisis(&lib, 3);
        }
        assert!(!st.events.scheduled.is_empty());
        let week = st.events.scheduled.iter().map(|s| s.week).min().unwrap();
        st.date = crate::sim::GameDate(week);
        st.maybe_trigger_event(&lib);
        assert!(st.events.pending.is_some());
        assert_eq!(lib.event(&st.events.pending.as_ref().unwrap().def_id).unwrap().kind, EventKind::Hotfix);
    }
}

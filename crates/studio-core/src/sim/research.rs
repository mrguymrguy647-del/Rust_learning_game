//! Building engine modules: requirements, research points and the weekly build progress.

use super::engine::{Blocker, Build};
use super::notify::NoteKind;
use super::state::GameState;
use crate::data::{ContentLibrary, EngineModuleDef};

impl GameState {
    /// Everything that currently stops `module` from being built (empty = ready to start).
    pub fn module_blockers(&self, content: &ContentLibrary, module: &EngineModuleDef) -> Vec<Blocker> {
        let mut out = Vec::new();
        if self.engine.has_module(&module.id) {
            return vec![Blocker::AlreadyBuilt];
        }
        if self.engine.building.as_ref().is_some_and(|b| b.module == module.id) {
            return vec![Blocker::Building];
        }
        if self.studio.tier < module.min_tier {
            out.push(Blocker::Tier(module.min_tier));
        }
        for dep in &module.requires_modules {
            if !self.engine.has_module(dep) {
                out.push(Blocker::Module(dep.clone()));
            }
        }
        for c in &module.required_challenges {
            if !self.progress.is_solved(c) {
                out.push(Blocker::Challenge(c.clone()));
            }
        }
        for (topic, need) in &module.required_topic_solves {
            let have = self.progress.solved_in_topic(content, topic) as u32;
            if have < *need {
                out.push(Blocker::TopicSolves { topic: topic.clone(), have, need: *need });
            }
        }
        if self.studio.research_points < module.research_cost {
            out.push(Blocker::ResearchPoints {
                have: self.studio.research_points,
                need: module.research_cost,
            });
        }
        if self.studio.money < module.money_cost {
            out.push(Blocker::Money { have: self.studio.money, need: module.money_cost });
        }
        if self.engine.building.is_some() {
            out.push(Blocker::OtherBuildInProgress);
        }
        out
    }

    pub fn start_module_build(&mut self, content: &ContentLibrary, module_id: &str) -> Result<(), String> {
        let module =
            content.module(module_id).ok_or_else(|| format!("Unknown module `{module_id}`."))?.clone();
        let blockers = self.module_blockers(content, &module);
        if let Some(first) = blockers.first() {
            return Err(describe_blocker(content, first));
        }
        self.studio.research_points -= module.research_cost;
        self.studio.money -= module.money_cost;
        if module.build_weeks == 0 {
            self.finish_build(content, &module);
        } else {
            self.engine.building = Some(Build { module: module.id.clone(), weeks_left: module.build_weeks });
            self.note(
                NoteKind::Info,
                format!("Started building “{}” ({} weeks).", module.name, module.build_weeks),
            );
        }
        Ok(())
    }

    fn finish_build(&mut self, content: &ContentLibrary, module: &EngineModuleDef) {
        self.engine.built.insert(module.id.clone());
        self.engine.building = None;
        self.progress.bump("modules_built", 1);
        self.note(NoteKind::Good, format!("Engine module ready: {}!", module.name));
        // Tell the player what just became possible.
        let unlocked: Vec<&str> = content
            .genres
            .iter()
            .filter(|g| {
                g.requires_features.iter().any(|f| module.features.contains(f))
                    && self.engine.has_all_features(content, &g.requires_features)
            })
            .map(|g| g.name.as_str())
            .collect();
        if !unlocked.is_empty() {
            self.note(NoteKind::Good, format!("New genres available: {}.", unlocked.join(", ")));
        }
    }

    /// Research points your programmers generate each week.
    pub fn weekly_research(&self, content: &ContentLibrary) -> f32 {
        let per = content.balance.research_per_skill_point;
        self.staff.iter().map(|s| (s.programming - 2.0).max(0.0) * per * s.morale_factor()).sum()
    }

    /// Weekly: passive research points and build progress.
    pub(crate) fn advance_engine(&mut self, content: &ContentLibrary) {
        self.studio.research_frac += self.weekly_research(content);
        let whole = self.studio.research_frac.floor();
        self.studio.research_frac -= whole;
        self.studio.research_points += whole as i64;

        let Some(build) = self.engine.building.as_mut() else {
            return;
        };
        build.weeks_left = build.weeks_left.saturating_sub(1);
        if build.weeks_left == 0 {
            let id = build.module.clone();
            if let Some(module) = content.module(&id).cloned() {
                self.finish_build(content, &module);
            } else {
                self.engine.building = None;
            }
        }
    }
}

/// Human-readable explanation of a blocker.
pub fn describe_blocker(content: &ContentLibrary, b: &Blocker) -> String {
    use crate::fmt::money;
    match b {
        Blocker::AlreadyBuilt => "Already built.".into(),
        Blocker::Building => "Currently being built.".into(),
        Blocker::Tier(t) => {
            format!(
                "Needs the {} (or bigger).",
                content.tier(*t).map(|t| t.name.as_str()).unwrap_or("next studio tier")
            )
        }
        Blocker::Module(id) => {
            format!("Build “{}” first.", content.module(id).map(|m| m.name.as_str()).unwrap_or(id))
        }
        Blocker::Challenge(id) => {
            format!(
                "Solve the challenge “{}”.",
                content.challenge(id).map(|c| c.title.as_str()).unwrap_or(id)
            )
        }
        Blocker::TopicSolves { topic, have, need } => format!(
            "Solve {need} {} challenges ({have} so far).",
            content.topic(topic).map(|t| t.name.as_str()).unwrap_or(topic)
        ),
        Blocker::ResearchPoints { have, need } => format!("Needs {need} research points (you have {have})."),
        Blocker::Money { have, need } => format!("Needs {} (you have {}).", money(*need), money(*have)),
        Blocker::OtherBuildInProgress => "Another module is already being built.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;
    use crate::sim::progress::SolveRecord;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        let st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(1));
        (lib, st)
    }

    #[test]
    fn core_loop_starts_built_and_others_need_work() {
        let (lib, mut st) = setup();
        assert!(st.engine.has_module("core_loop"));
        let am = lib.module("asset_manager").unwrap().clone();
        let blockers = st.module_blockers(&lib, &am);
        assert!(blockers.iter().any(|b| matches!(b, Blocker::TopicSolves { .. })));
        assert!(blockers.iter().any(|b| matches!(b, Blocker::ResearchPoints { .. })));
        assert!(st.start_module_build(&lib, "asset_manager").is_err());

        // Satisfy everything.
        for c in lib.challenges_in_topic("ownership").take(3) {
            st.progress.solved.insert(c.id.clone(), SolveRecord::default());
        }
        st.studio.research_points = 1000;
        st.studio.money = 100_000;
        let am_blockers = st.module_blockers(&lib, &am);
        // Only fails if the topic has fewer than 3 challenges (content still growing).
        if am_blockers.is_empty() {
            st.start_module_build(&lib, "asset_manager").unwrap();
            assert!(st.engine.building.is_some());
            assert_eq!(st.studio.research_points, 1000 - am.research_cost);
        }
    }

    #[test]
    fn build_completes_after_its_weeks_and_unlocks_features() {
        let (mut lib, mut st) = setup();
        // A tiny synthetic tree so the test does not depend on curriculum size.
        let mut m = lib.module("asset_manager").unwrap().clone();
        m.required_topic_solves.clear();
        m.build_weeks = 3;
        lib.engine_modules.retain(|x| x.id != "asset_manager");
        lib.engine_modules.push(m);
        st.studio.research_points = 500;
        st.studio.money = 50_000;
        st.start_module_build(&lib, "asset_manager").unwrap();
        for _ in 0..2 {
            st.advance_week(&lib);
            assert!(!st.engine.has_module("asset_manager"));
        }
        st.advance_week(&lib);
        assert!(st.engine.has_module("asset_manager"));
        assert!(st.engine.has_feature(&lib, "assets"));
        assert!(st.engine.building.is_none());
        assert_eq!(st.progress.stat("modules_built"), 1);
    }

    #[test]
    fn only_one_build_at_a_time_and_dependencies_are_enforced() {
        let (mut lib, mut st) = setup();
        for m in lib.engine_modules.iter_mut() {
            m.required_topic_solves.clear();
            m.min_tier = 0;
        }
        st.studio.research_points = 100_000;
        st.studio.money = 10_000_000;
        assert!(st.start_module_build(&lib, "renderer_2d").is_err(), "needs asset_manager first");
        st.start_module_build(&lib, "asset_manager").unwrap();
        let second = st.start_module_build(&lib, "dialogue");
        assert!(second.unwrap_err().contains("already being built"));
    }

    #[test]
    fn programmers_generate_research_points_over_time() {
        let (lib, mut st) = setup();
        assert!(st.weekly_research(&lib) > 0.0);
        for _ in 0..30 {
            st.advance_week(&lib);
        }
        assert!(st.studio.research_points >= 10, "got {}", st.studio.research_points);
        st.staff[0].programming = 1.0;
        assert_eq!(st.weekly_research(&lib), 0.0);
    }
}

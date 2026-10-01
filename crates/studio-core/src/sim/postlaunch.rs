//! Life after release: marketing, patches, DLC, sequels and engine licensing.

use serde::{Deserialize, Serialize};

use super::economy::WeekFinance;
use super::model::ProjectSize;
use super::notify::NoteKind;
use super::project::{Blocker, Project, ProjectConfig};
use super::state::GameState;
use crate::data::ContentLibrary;

/// A patch being worked on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub game_id: u32,
    pub weeks_left: u32,
}

/// Another studio paying to use your engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct License {
    pub licensee: String,
    pub weekly_fee: i64,
    pub weeks_left: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseOffer {
    pub licensee: String,
    pub weekly_fee: i64,
    pub term_weeks: u32,
    pub signing_fee: i64,
}

const LICENSEES: [&str; 12] = [
    "Mudlark Games",
    "Tin Can Interactive",
    "Paper Moon Studio",
    "Overclocked Oats",
    "Fjord & Fox",
    "Gearhead Labs",
    "Blue Lantern",
    "Hollow Oak",
    "Static Sparrow",
    "Pocket Comet",
    "Velvet Anvil",
    "Quiet Thunder",
];

pub const MAX_PATCHES: u32 = 3;
pub const MAX_DLC: u32 = 2;

impl GameState {
    // ---- marketing -------------------------------------------------------------------------------

    /// Buy a marketing campaign for the project in development.
    pub fn buy_marketing(&mut self, content: &ContentLibrary, id: &str) -> Result<f32, String> {
        let m = content.marketing.iter().find(|m| m.id == id).ok_or("Unknown campaign.")?.clone();
        if self.studio.tier < m.min_tier {
            return Err("Your studio is too small for that campaign.".into());
        }
        if self.studio.money < m.cost {
            return Err(format!("That costs {}.", crate::fmt::money(m.cost)));
        }
        let Some(p) = self.project.as_mut() else {
            return Err("Start a project first: marketing promotes the game you are making.".into());
        };
        if p.hype >= 99.0 {
            return Err("Your hype is already at the maximum.".into());
        }
        let gain = m.hype * (1.0 - p.hype / 130.0).max(0.2);
        p.hype = (p.hype + gain).min(100.0);
        p.cost_so_far += m.cost;
        self.studio.money -= m.cost;
        self.progress.bump("campaigns_run", 1);
        Ok(gain)
    }

    // ---- patches ---------------------------------------------------------------------------------

    pub fn patch_cost(&self, content: &ContentLibrary, game_id: u32) -> i64 {
        let Some(g) = self.games.iter().find(|g| g.id == game_id) else {
            return 0;
        };
        let budget = content.balance.size(g.size).budget;
        500 + (budget as f32 * 0.08 * (1 + g.patches) as f32).round() as i64
    }

    pub fn patch_problem(&self, content: &ContentLibrary, game_id: u32) -> Option<String> {
        let Some(g) = self.games.iter().find(|g| g.id == game_id) else {
            return Some("Unknown game.".into());
        };
        if g.is_dlc {
            return Some("Patch the main game instead.".into());
        }
        if self.patch.is_some() {
            return Some("A patch is already in the works.".into());
        }
        if g.patches >= MAX_PATCHES {
            return Some("There is nothing left to patch.".into());
        }
        if g.bugs < 2.0 {
            return Some("The game is already clean.".into());
        }
        if self.studio.money < self.patch_cost(content, game_id) {
            return Some(format!("A patch costs {}.", crate::fmt::money(self.patch_cost(content, game_id))));
        }
        None
    }

    pub fn start_patch(&mut self, content: &ContentLibrary, game_id: u32) -> Result<(), String> {
        if let Some(problem) = self.patch_problem(content, game_id) {
            return Err(problem);
        }
        let cost = self.patch_cost(content, game_id);
        let weeks = match self.games.iter().find(|g| g.id == game_id).map(|g| g.size) {
            Some(ProjectSize::Small) => 2,
            Some(ProjectSize::Medium) | Some(ProjectSize::Large) => 3,
            _ => 4,
        };
        self.studio.money -= cost;
        self.patch = Some(Patch { game_id, weeks_left: weeks });
        let name = self.games.iter().find(|g| g.id == game_id).map(|g| g.name.clone()).unwrap_or_default();
        self.note(NoteKind::Info, format!("Working on a patch for “{name}” ({weeks} weeks)."));
        Ok(())
    }

    pub(crate) fn advance_patch(&mut self) {
        let Some(p) = self.patch.as_mut() else {
            return;
        };
        p.weeks_left = p.weeks_left.saturating_sub(1);
        if p.weeks_left > 0 {
            return;
        }
        let id = p.game_id;
        self.patch = None;
        if let Some(g) = self.games.iter_mut().find(|g| g.id == id) {
            g.patches += 1;
            g.bugs = (g.bugs - 10.0).max(0.0);
            g.metascore = (g.metascore + 1.5).min(100.0);
            g.decay = (g.decay + 0.015).min(0.95);
            g.on_sale = true;
            g.boost_weeks = 3;
            g.boost_mult = 1.3;
            let name = g.name.clone();
            self.studio.reputation += 30;
            self.note(
                NoteKind::Good,
                format!("The patch for “{name}” is out: fewer bugs and a little sales bump."),
            );
        }
    }

    // ---- DLC & sequels ----------------------------------------------------------------------------

    pub fn dlc_problem(&self, game_id: u32) -> Option<String> {
        let Some(g) = self.games.iter().find(|g| g.id == game_id) else {
            return Some("Unknown game.".into());
        };
        if g.is_dlc {
            return Some("DLC cannot have DLC of its own.".into());
        }
        if self.project.is_some() {
            return Some("Finish or cancel the current project first.".into());
        }
        if g.metascore < 60.0 {
            return Some("Players need to love the game first (Metascore 60+).".into());
        }
        if g.age(self.date.week()) < 8 {
            return Some("Wait a couple of months after release.".into());
        }
        if g.dlc_count >= MAX_DLC {
            return Some("You have already released all the DLC players want.".into());
        }
        None
    }

    pub fn start_dlc(&mut self, content: &ContentLibrary, game_id: u32) -> Result<(), String> {
        if let Some(p) = self.dlc_problem(game_id) {
            return Err(p);
        }
        let Some(g) = self.games.iter().find(|g| g.id == game_id).cloned() else {
            return Err("Unknown game.".into());
        };
        let size = content.balance.size(g.size);
        let id = self.next_id();
        let name = format!("{}: Expansion {}", g.name, g.dlc_count + 1);
        let platform_cost = 0;
        self.project = Some(Project {
            id,
            name: name.clone(),
            genre: g.genre.clone(),
            theme: g.theme.clone(),
            platform: g.platform.clone(),
            audience: g.audience,
            size: g.size,
            focus: g.categories.normalized_to(100.0),
            started_week: self.date.week(),
            work_total: size.work * 0.3,
            blockers: vec![Blocker { at: 0.5, resolved: false, challenge_id: None }],
            hype: 15.0,
            budget_total: size.budget / 5,
            cost_so_far: platform_cost,
            dlc_for: Some(game_id),
            ..Project::default()
        });
        self.note(NoteKind::Info, format!("Started work on “{name}”."));
        Ok(())
    }

    /// A pre-filled configuration for a sequel (the player can still tweak it).
    pub fn sequel_config(&self, game_id: u32) -> Option<ProjectConfig> {
        let g = self.games.iter().find(|g| g.id == game_id && !g.is_dlc)?;
        let name = match g
            .name
            .rsplit_once(' ')
            .and_then(|(head, tail)| tail.parse::<u32>().ok().map(|n| (head, n)))
        {
            Some((head, n)) => format!("{head} {}", n + 1),
            None => format!("{} 2", g.name),
        };
        Some(ProjectConfig {
            name,
            genre: g.genre.clone(),
            theme: g.theme.clone(),
            platform: g.platform.clone(),
            audience: g.audience,
            size: g.size,
            focus: g.categories.normalized_to(100.0),
            sequel_of: Some(g.id),
        })
    }

    // ---- engine licensing ---------------------------------------------------------------------------

    /// How valuable your engine is to other studios.
    pub fn engine_value(&self, content: &ContentLibrary) -> i64 {
        self.engine.modules(content).map(|m| m.research_cost + m.money_cost / 200).sum()
    }

    pub fn license_slots(&self, content: &ContentLibrary) -> usize {
        content.tier(self.studio.tier).map(|t| t.license_slots).unwrap_or(0)
    }

    pub fn license_income(&self) -> i64 {
        self.licenses.iter().map(|l| l.weekly_fee).sum()
    }

    pub fn accept_license(&mut self, content: &ContentLibrary, index: usize) -> Result<(), String> {
        if self.licenses.len() >= self.license_slots(content) {
            return Err("No free licensing slots. A bigger studio can support more licensees.".into());
        }
        if index >= self.license_offers.len() {
            return Err("That offer expired.".into());
        }
        let o = self.license_offers.remove(index);
        self.studio.money += o.signing_fee;
        self.note(
            NoteKind::Good,
            format!(
                "{} licensed your engine: {}/week for {} weeks.",
                o.licensee,
                crate::fmt::money(o.weekly_fee),
                o.term_weeks
            ),
        );
        self.licenses.push(License {
            licensee: o.licensee,
            weekly_fee: o.weekly_fee,
            weeks_left: o.term_weeks,
        });
        self.progress.bump("licenses_signed", 1);
        Ok(())
    }

    pub fn decline_license(&mut self, index: usize) {
        if index < self.license_offers.len() {
            self.license_offers.remove(index);
        }
    }

    /// Weekly: collect fees, expire old licences, and occasionally attract new offers.
    pub(crate) fn advance_licensing(&mut self, content: &ContentLibrary, fin: &mut WeekFinance) {
        let mult = self.difficulty.revenue_mult();
        let mut expired: Vec<String> = Vec::new();
        for l in self.licenses.iter_mut() {
            fin.revenue += (l.weekly_fee as f32 * mult).round() as i64;
            l.weeks_left = l.weeks_left.saturating_sub(1);
            if l.weeks_left == 0 {
                expired.push(l.licensee.clone());
            }
        }
        self.licenses.retain(|l| l.weeks_left > 0);
        for name in expired {
            self.note(NoteKind::Info, format!("{name}'s engine licence ended."));
        }

        if self.date.is_month_start() {
            self.license_offers.truncate(2);
            let value = self.engine_value(content);
            let slots = self.license_slots(content);
            if value >= 150
                && self.licenses.len() < slots
                && self.license_offers.len() < 3
                && self.rng.chance(0.6)
            {
                let name = self.rng.pick(&LICENSEES).copied().unwrap_or("Some Studio");
                let taken = self.licenses.iter().any(|l| l.licensee == name)
                    || self.license_offers.iter().any(|o| o.licensee == name);
                if !taken {
                    let weekly_fee = (value as f32 * 1.4 * self.rng.range_f32(0.8, 1.25)).round() as i64;
                    let term_weeks = self.rng.range_i64(26, 104) as u32;
                    self.license_offers.push(LicenseOffer {
                        licensee: name.to_string(),
                        weekly_fee,
                        term_weeks,
                        signing_fee: weekly_fee * 6,
                    });
                    self.note(NoteKind::Info, format!("{name} is interested in licensing your engine."));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;
    use crate::sim::ReleasedGame;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        let mut st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(4));
        st.studio.money = 5_000_000;
        (lib, st)
    }

    fn game(st: &mut GameState, id: u32, meta: f32, bugs: f32, age: u32) {
        let week = st.date.week();
        st.date.advance(age);
        st.games.push(ReleasedGame {
            id,
            name: format!("Hit {id}"),
            metascore: meta,
            bugs,
            release_week: week,
            launch_units: 1000.0,
            on_sale: false,
            units_total: 20_000,
            ..Default::default()
        });
    }

    #[test]
    fn marketing_raises_hype_with_diminishing_returns() {
        let (lib, mut st) = setup();
        assert!(st.buy_marketing(&lib, "trailer").is_err(), "needs a project");
        let genre = lib.genres.first().unwrap();
        st.start_project(
            &lib,
            ProjectConfig {
                name: "X".into(),
                genre: genre.id.clone(),
                theme: "cooking".into(),
                platform: "pc".into(),
                audience: crate::sim::Audience::Everyone,
                size: ProjectSize::Small,
                focus: genre.ideal,
                sequel_of: None,
            },
        )
        .unwrap();
        let money = st.studio.money;
        let first = st.buy_marketing(&lib, "trailer").unwrap();
        let second = st.buy_marketing(&lib, "trailer").unwrap();
        assert!(first > second && second > 0.0);
        assert_eq!(money - st.studio.money, 8000);
        for _ in 0..30 {
            let _ = st.buy_marketing(&lib, "trailer");
        }
        assert!(st.project.as_ref().unwrap().hype <= 100.0);
        assert!(st.buy_marketing(&lib, "tv_campaign").is_err(), "needs a bigger studio");
    }

    #[test]
    fn patches_cost_money_take_time_and_fix_bugs_up_to_a_limit() {
        let (lib, mut st) = setup();
        game(&mut st, 5, 70.0, 25.0, 4);
        let before = st.games[0].clone();
        st.start_patch(&lib, 5).unwrap();
        assert!(st.start_patch(&lib, 5).unwrap_err().contains("already"));
        for _ in 0..2 {
            st.advance_week_auto(&lib);
        }
        let after = &st.games[0];
        assert_eq!(after.patches, 1);
        assert!(after.bugs < before.bugs && after.metascore > before.metascore && after.decay > before.decay);
        assert!(after.on_sale, "a patch revives sales");
        let c1 = st.patch_cost(&lib, 5);
        st.games[0].patches = 2;
        assert!(st.patch_cost(&lib, 5) > c1, "later patches cost more");
        st.games[0].patches = MAX_PATCHES;
        assert!(st.patch_problem(&lib, 5).is_some());
    }

    #[test]
    fn dlc_needs_a_good_old_game_and_creates_a_small_project() {
        let (lib, mut st) = setup();
        game(&mut st, 5, 50.0, 5.0, 20);
        assert!(st.dlc_problem(5).unwrap().contains("Metascore"));
        st.games[0].metascore = 75.0;
        st.games[0].release_week = st.date.week();
        assert!(st.dlc_problem(5).unwrap().contains("couple of months"));
        st.date.advance(10);
        assert!(st.dlc_problem(5).is_none());
        st.start_dlc(&lib, 5).unwrap();
        let p = st.project.as_ref().unwrap();
        assert_eq!(p.dlc_for, Some(5));
        assert!(p.work_total < lib.balance.size(p.size).work);
        assert!(st.dlc_problem(5).is_some(), "only one project at a time");
    }

    #[test]
    fn dlc_release_adds_a_separate_selling_entry() {
        let (lib, mut st) = setup();
        game(&mut st, 5, 80.0, 4.0, 12);
        st.start_dlc(&lib, 5).unwrap();
        let mut guard = 0;
        while st.project.as_ref().is_some_and(|p| !p.is_complete() || p.open_blockers() > 0) && guard < 300 {
            guard += 1;
            if let Some(a) = st.pending_attempt.clone() {
                let c = lib.challenge(&a.challenge_id).unwrap().clone();
                let mut d = a.clone();
                d.submissions = 1;
                st.complete_challenge(&lib, &c, &d);
                st.pending_attempt = None;
                continue;
            }
            st.advance_week_auto(&lib);
        }
        let idx = st.release_project(&lib).unwrap();
        let dlc = &st.games[idx];
        assert!(dlc.is_dlc && dlc.parent == Some(5));
        assert!(dlc.reviews.is_empty());
        assert!(dlc.launch_units > 100.0);
        assert_eq!(st.games[0].dlc_count, 1);
    }

    #[test]
    fn sequels_are_prefilled_and_carry_the_fanbase() {
        let (lib, mut st) = setup();
        game(&mut st, 5, 85.0, 4.0, 12);
        st.games[0].genre = "puzzle".into();
        st.games[0].theme = "cooking".into();
        st.games[0].categories = lib.genre("puzzle").unwrap().ideal;
        st.games[0].name = "Hit Parade".into();
        let cfg = st.sequel_config(5).unwrap();
        assert_eq!(cfg.name, "Hit Parade 2");
        assert_eq!(cfg.sequel_of, Some(5));
        st.games[0].name = "Hit Parade 2".into();
        assert_eq!(st.sequel_config(5).unwrap().name, "Hit Parade 3");
        st.start_project(&lib, cfg).unwrap();
        assert!(st.project.as_ref().unwrap().hype > 0.0 || st.studio.reputation == 0);
        assert_eq!(st.project.as_ref().unwrap().sequel_of, Some(5));
    }

    #[test]
    fn engine_licensing_creates_passive_income() {
        let (lib, mut st) = setup();
        st.studio.tier = 2;
        for id in [
            "core_loop",
            "asset_manager",
            "renderer_2d",
            "input_states",
            "save_system",
            "inventory",
            "dialogue",
        ] {
            st.engine.built.insert(id.into());
        }
        assert!(st.engine_value(&lib) >= 150);
        assert!(st.accept_license(&lib, 0).is_err(), "no offers yet");
        for _ in 0..52 {
            st.advance_week_auto(&lib);
            if !st.license_offers.is_empty() {
                break;
            }
        }
        assert!(!st.license_offers.is_empty(), "offers appear for a valuable engine");
        let signing = st.license_offers[0].signing_fee;
        let fee = st.license_offers[0].weekly_fee;
        let money = st.studio.money;
        st.accept_license(&lib, 0).unwrap();
        assert_eq!(st.studio.money, money + signing);
        assert_eq!(st.license_income(), fee);
        let before = st.finances.lifetime_revenue;
        st.advance_week_auto(&lib);
        assert!(st.finances.lifetime_revenue >= before + fee);
    }

    #[test]
    fn licence_slots_are_limited_by_the_studio_tier() {
        let (lib, mut st) = setup();
        st.studio.tier = 0;
        assert_eq!(st.license_slots(&lib), 0);
        st.license_offers.push(LicenseOffer {
            licensee: "X".into(),
            weekly_fee: 100,
            term_weeks: 10,
            signing_fee: 600,
        });
        assert!(st.accept_license(&lib, 0).unwrap_err().contains("slots"));
    }
}

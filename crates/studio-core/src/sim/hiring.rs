//! Staff management: candidates, hiring, firing, training, morale and seniority.

use super::notify::NoteKind;
use super::staff::{Skill, Staff, Training, MAX_SKILL};
use super::state::GameState;
use crate::data::ContentLibrary;

/// Highest craft skill a fresh candidate can have per studio tier.
const TIER_SKILL_CAP: [f32; 5] = [5.0, 6.5, 8.0, 9.0, 10.0];

/// Weekly salary for a person with these skills.
pub fn salary_for(s: &Staff) -> i64 {
    let total = s.programming + s.design + s.art + s.audio + s.speed * 0.6;
    (250.0 + 55.0 * total).round() as i64
}

fn round_half(x: f32) -> f32 {
    ((x * 2.0).round() / 2.0).clamp(1.0, MAX_SKILL)
}

/// Cost of a training course for `skill`.
pub fn training_cost(s: &Staff, skill: Skill) -> i64 {
    (300.0 * s.skill(skill).max(1.0).powf(1.3)).round() as i64
}

pub fn training_weeks(content: &ContentLibrary, s: &Staff, skill: Skill) -> u32 {
    let mult: f32 = s.traits.iter().filter_map(|t| content.trait_def(t)).map(|t| t.training_mult).product();
    ((2.0 + s.skill(skill) / 2.5) * mult).round().max(1.0) as u32
}

impl GameState {
    pub fn staff_slots(&self, content: &ContentLibrary) -> usize {
        content.tier(self.studio.tier).map(|t| t.staff_slots).unwrap_or(1)
    }

    /// A random job candidate suited to the studio's current tier.
    pub fn generate_candidate(&mut self, content: &ContentLibrary) -> Staff {
        let cap = TIER_SKILL_CAP[self.studio.tier.min(TIER_SKILL_CAP.len() - 1)];
        let first = self.rng.pick(&content.names.first).cloned().unwrap_or_else(|| "Sam".into());
        let last = self.rng.pick(&content.names.last).cloned().unwrap_or_else(|| "Dev".into());
        let id = self.next_id();
        let mut s = Staff {
            id,
            name: format!("{first} {last}"),
            morale: 70.0,
            hired_week: self.date.week(),
            ..Staff::default()
        };

        // One specialty, one secondary skill, the rest weak.
        let specialty = self.rng.range_usize(0, 3);
        let mut secondary = self.rng.range_usize(0, 3);
        if secondary == specialty {
            secondary = (secondary + 1) % 4;
        }
        let craft = [Skill::Programming, Skill::Design, Skill::Art, Skill::Audio];
        for (i, skill) in craft.iter().enumerate() {
            let v = if i == specialty {
                self.rng.range_f32((cap - 2.5).max(2.0), cap)
            } else if i == secondary {
                self.rng.range_f32((cap - 4.5).max(1.5), (cap - 2.0).max(2.5))
            } else {
                self.rng.range_f32(1.0, (cap - 3.0).clamp(2.0, 4.0))
            };
            match skill {
                Skill::Programming => s.programming = round_half(v),
                Skill::Design => s.design = round_half(v),
                Skill::Art => s.art = round_half(v),
                Skill::Audio => s.audio = round_half(v),
                Skill::Speed => {}
            }
        }
        s.speed = round_half(self.rng.range_f32(3.0, 8.0));
        if self.rng.chance(0.4) {
            let ids: Vec<&String> = content.traits.iter().map(|t| &t.id).collect();
            if let Some(t) = self.rng.pick(&ids) {
                s.traits.push((*t).clone());
            }
        }
        s.salary = salary_for(&s);
        s
    }

    /// Replace the candidate pool with fresh faces.
    pub fn refresh_candidates(&mut self, content: &ContentLibrary) {
        let n = self.rng.range_usize(4, 6);
        self.candidates = (0..n).map(|_| self.generate_candidate(content)).collect();
    }

    pub fn hire_fee(&self, content: &ContentLibrary, candidate: &Staff) -> i64 {
        (candidate.salary as f32 * content.balance.hire_fee_weeks).round() as i64
    }

    pub fn hire(&mut self, content: &ContentLibrary, candidate_id: u32) -> Result<(), String> {
        let Some(pos) = self.candidates.iter().position(|c| c.id == candidate_id) else {
            return Err("That candidate is no longer available.".into());
        };
        if self.staff.len() >= self.staff_slots(content) {
            return Err("No free desks. Upgrade your studio to hire more people.".into());
        }
        let fee = self.hire_fee(content, &self.candidates[pos]);
        if self.studio.money < fee {
            return Err(format!("The recruiting fee is {}.", crate::fmt::money(fee)));
        }
        let mut person = self.candidates.remove(pos);
        person.hired_week = self.date.week();
        self.studio.money -= fee;
        self.note(NoteKind::Good, format!("{} joined the team.", person.name));
        self.staff.push(person);
        self.progress.set_max("team_size", self.staff.len() as i64);
        Ok(())
    }

    /// Dismiss someone. Costs four weeks of severance. The founder cannot be fired.
    pub fn fire(&mut self, staff_id: u32) -> Result<i64, String> {
        let Some(pos) = self.staff.iter().position(|s| s.id == staff_id) else {
            return Err("No such employee.".into());
        };
        if self.staff[pos].is_founder {
            return Err("You cannot fire yourself.".into());
        }
        let severance = self.staff[pos].salary * 4;
        let person = self.staff.remove(pos);
        self.studio.money -= severance;
        for s in self.staff.iter_mut() {
            s.morale = (s.morale - 4.0).max(0.0);
        }
        self.note(NoteKind::Warn, format!("{} was let go.", person.name));
        Ok(severance)
    }

    pub fn start_training(
        &mut self,
        content: &ContentLibrary,
        staff_id: u32,
        skill: Skill,
    ) -> Result<(), String> {
        let Some(pos) = self.staff.iter().position(|s| s.id == staff_id) else {
            return Err("No such employee.".into());
        };
        let s = &self.staff[pos];
        if s.training.is_some() {
            return Err(format!("{} is already in training.", s.name));
        }
        if s.skill(skill) >= MAX_SKILL {
            return Err("Already a master of that skill.".into());
        }
        let cost = training_cost(s, skill);
        if self.studio.money < cost {
            return Err(format!("The course costs {}.", crate::fmt::money(cost)));
        }
        let weeks = training_weeks(content, s, skill);
        self.studio.money -= cost;
        let name = self.staff[pos].name.clone();
        self.staff[pos].training = Some(Training { skill, weeks_left: weeks });
        self.note(
            NoteKind::Info,
            format!("{name} started a {} course ({weeks} weeks).", skill.label().to_lowercase()),
        );
        Ok(())
    }

    /// One week's salary as a bonus: morale +20.
    pub fn give_bonus(&mut self, staff_id: u32) -> Result<(), String> {
        let Some(s) = self.staff.iter().position(|s| s.id == staff_id) else {
            return Err("No such employee.".into());
        };
        let cost = (self.staff[s].salary * 2).max(200);
        if self.studio.money < cost {
            return Err(format!("A bonus costs {}.", crate::fmt::money(cost)));
        }
        self.studio.money -= cost;
        self.staff[s].morale = (self.staff[s].morale + 20.0).min(100.0);
        Ok(())
    }

    /// +10 % salary for good: morale +12.
    pub fn give_raise(&mut self, staff_id: u32) -> Result<(), String> {
        let Some(s) = self.staff.iter_mut().find(|s| s.id == staff_id) else {
            return Err("No such employee.".into());
        };
        if s.is_founder {
            return Err("You already pay yourself nothing, congratulations.".into());
        }
        s.salary = (s.salary as f32 * 1.1).round() as i64;
        s.morale = (s.morale + 12.0).min(100.0);
        Ok(())
    }

    pub fn party_cost(&self) -> i64 {
        250 * self.staff.len() as i64
    }

    pub fn throw_party(&mut self) -> Result<(), String> {
        let cost = self.party_cost();
        if self.studio.money < cost {
            return Err(format!("A team party costs {}.", crate::fmt::money(cost)));
        }
        self.studio.money -= cost;
        for s in self.staff.iter_mut() {
            s.morale = (s.morale + 10.0).min(100.0);
        }
        self.note(NoteKind::Good, "The team had a great party.");
        Ok(())
    }

    // ---- seniority ----------------------------------------------------------------------

    pub fn senior_count(&self, content: &ContentLibrary) -> usize {
        self.staff
            .iter()
            .filter(|s| {
                s.is_senior() || s.traits.iter().any(|t| content.trait_def(t).is_some_and(|d| d.free_hint))
            })
            .count()
    }

    /// Hint tiers the senior staff cover for free (0..=3).
    pub fn free_hints(&self, content: &ContentLibrary) -> usize {
        self.senior_count(content).min(3)
    }

    /// Contractor discount from experienced staff: 15 % each, at most 45 %.
    pub fn contractor_discount(&self, content: &ContentLibrary) -> f32 {
        0.15 * self.senior_count(content).min(3) as f32
    }

    // ---- weekly -----------------------------------------------------------------------------

    pub(crate) fn advance_staff(&mut self, content: &ContentLibrary) {
        let week = self.date.week();

        // Training progress.
        let mut finished: Vec<(String, Skill)> = Vec::new();
        for s in self.staff.iter_mut() {
            if let Some(t) = s.training.as_mut() {
                t.weeks_left = t.weeks_left.saturating_sub(1);
                if t.weeks_left == 0 {
                    let skill = t.skill;
                    s.training = None;
                    s.raise(skill, 1.0);
                    s.morale = (s.morale + 5.0).min(100.0);
                    finished.push((s.name.clone(), skill));
                }
            }
        }
        for (name, skill) in finished {
            self.note(
                NoteKind::Good,
                format!("{name} finished the {} course and improved!", skill.label().to_lowercase()),
            );
        }

        // Experience: skills creep up with years on the job.
        let mut grown: Vec<String> = Vec::new();
        for s in self.staff.iter_mut() {
            if s.experience_weeks > 0 && s.experience_weeks.is_multiple_of(40) {
                let mut best = Skill::Programming;
                for k in [Skill::Design, Skill::Art, Skill::Audio] {
                    if s.skill(k) > s.skill(best) {
                        best = k;
                    }
                }
                if s.skill(best) < MAX_SKILL {
                    s.raise(best, 0.5);
                    grown.push(s.name.clone());
                }
                s.experience_weeks += 1; // avoid re-triggering on the same count
            }
        }
        for name in grown {
            self.note(NoteKind::Info, format!("{name} gained experience."));
        }

        // Morale effects from traits.
        let team_boost: f32 = self
            .staff
            .iter()
            .flat_map(|s| s.traits.iter())
            .filter_map(|t| content.trait_def(t))
            .map(|t| t.team_morale)
            .sum();
        for s in self.staff.iter_mut() {
            let own: f32 = s.traits.iter().filter_map(|t| content.trait_def(t)).map(|t| t.morale_delta).sum();
            s.morale = (s.morale + own + team_boost).clamp(0.0, 100.0);
        }

        // Unhappy people quit.
        let mut quitters: Vec<u32> = Vec::new();
        for s in self.staff.iter() {
            if !s.is_founder && s.morale < 25.0 && self.rng.chance(0.08) {
                quitters.push(s.id);
            }
        }
        for id in quitters {
            if let Some(pos) = self.staff.iter().position(|s| s.id == id) {
                let person = self.staff.remove(pos);
                self.note(NoteKind::Bad, format!("{} quit because morale was too low.", person.name));
            }
        }

        // New faces every few weeks.
        let every = content.balance.candidate_refresh_weeks.max(1);
        if self.candidates.is_empty() || week.is_multiple_of(every) {
            self.refresh_candidates(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        let mut st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(5));
        st.studio.money = 100_000;
        st.refresh_candidates(&lib);
        (lib, st)
    }

    #[test]
    fn candidates_respect_tier_caps_and_have_fair_salaries() {
        let (lib, mut st) = setup();
        for _ in 0..200 {
            let c = st.generate_candidate(&lib);
            assert!(c.programming <= 5.0 && c.design <= 5.0 && c.art <= 5.0 && c.audio <= 5.0, "{c:?}");
            assert!(c.programming >= 1.0 && c.speed >= 3.0);
            assert_eq!(c.salary, salary_for(&c));
            assert!(c.salary > 300 && c.salary < 3000);
            assert!(!c.is_founder);
        }
        st.studio.tier = 4;
        let strong = (0..200)
            .map(|_| st.generate_candidate(&lib))
            .map(|c| c.programming.max(c.design).max(c.art).max(c.audio))
            .fold(0.0, f32::max);
        assert!(strong >= 8.0, "high tiers attract strong candidates, best was {strong}");
    }

    #[test]
    fn hiring_needs_a_desk_and_money() {
        let (lib, mut st) = setup();
        let id = st.candidates[0].id;
        // The bedroom has a single slot, taken by the founder.
        assert!(st.hire(&lib, id).unwrap_err().contains("desk"));
        st.studio.tier = 1;
        let fee = st.hire_fee(&lib, &st.candidates[0]);
        let before = st.studio.money;
        st.hire(&lib, id).unwrap();
        assert_eq!(st.staff.len(), 2);
        assert_eq!(before - st.studio.money, fee);
        assert!(st.candidates.iter().all(|c| c.id != id));
        st.studio.money = 0;
        let id2 = st.candidates[0].id;
        assert!(st.hire(&lib, id2).unwrap_err().contains("fee"));
    }

    #[test]
    fn firing_costs_severance_and_not_the_founder() {
        let (lib, mut st) = setup();
        st.studio.tier = 1;
        let id = st.candidates[0].id;
        st.hire(&lib, id).unwrap();
        let salary = st.staff[1].salary;
        let money = st.studio.money;
        assert_eq!(st.fire(id).unwrap(), salary * 4);
        assert_eq!(st.studio.money, money - salary * 4);
        assert_eq!(st.staff.len(), 1);
        let founder = st.staff[0].id;
        assert!(st.fire(founder).is_err());
    }

    #[test]
    fn training_takes_weeks_costs_money_and_raises_the_skill() {
        let (lib, mut st) = setup();
        let id = st.staff[0].id;
        let before = st.staff[0].art;
        let money = st.studio.money;
        let cost = training_cost(&st.staff[0], Skill::Art);
        st.start_training(&lib, id, Skill::Art).unwrap();
        assert_eq!(money - st.studio.money, cost);
        assert!(st.start_training(&lib, id, Skill::Design).is_err(), "one course at a time");
        let weeks = st.staff[0].training.as_ref().unwrap().weeks_left;
        for _ in 0..weeks {
            st.advance_week(&lib);
        }
        assert!(st.staff[0].training.is_none());
        assert!((st.staff[0].art - (before + 1.0)).abs() < 1e-4);
    }

    #[test]
    fn unhappy_staff_eventually_quit_but_never_the_founder() {
        let (lib, mut st) = setup();
        st.studio.tier = 2;
        for _ in 0..3 {
            let id = st.candidates[0].id;
            st.hire(&lib, id).unwrap();
            st.refresh_candidates(&lib);
        }
        assert_eq!(st.staff.len(), 4);
        for _ in 0..400 {
            for s in st.staff.iter_mut() {
                s.morale = 5.0;
            }
            st.studio.money = 1_000_000;
            st.advance_week(&lib);
        }
        assert_eq!(st.staff.len(), 1, "everyone but the founder quit");
        assert!(st.staff[0].is_founder);
    }

    #[test]
    fn seniors_give_free_hints_and_cheaper_contractors() {
        let (lib, mut st) = setup();
        assert_eq!(st.free_hints(&lib), 0);
        assert_eq!(st.contractor_discount(&lib), 0.0);
        st.staff[0].programming = 8.0;
        assert_eq!(st.free_hints(&lib), 1);
        assert!((st.contractor_discount(&lib) - 0.15).abs() < 1e-6);
        let mut mentor = Staff { id: 99, ..Staff::default() };
        mentor.traits.push("mentor".into());
        st.staff.push(mentor);
        assert_eq!(st.free_hints(&lib), 2);
        for i in 0..5 {
            st.staff.push(Staff { id: 100 + i, programming: 9.0, ..Staff::default() });
        }
        assert_eq!(st.free_hints(&lib), 3, "capped");
        assert!(st.contractor_discount(&lib) <= 0.45 + 1e-6);
    }

    #[test]
    fn morale_perks_cost_money() {
        let (_lib, mut st) = setup();
        st.staff[0].morale = 30.0;
        let money = st.studio.money;
        st.give_bonus(st.staff[0].id).unwrap();
        assert!(st.staff[0].morale >= 50.0 && st.studio.money < money);
        st.throw_party().unwrap();
        assert!(st.staff[0].morale >= 60.0);
    }
}

//! Employees: stats, output, morale, seniority.

use serde::{Deserialize, Serialize};

use super::model::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Skill {
    Programming,
    Design,
    Art,
    Audio,
    Speed,
}

impl Skill {
    pub const ALL: [Skill; 5] = [Skill::Programming, Skill::Design, Skill::Art, Skill::Audio, Skill::Speed];

    pub fn label(self) -> &'static str {
        match self {
            Skill::Programming => "Programming",
            Skill::Design => "Design",
            Skill::Art => "Art",
            Skill::Audio => "Audio",
            Skill::Speed => "Speed",
        }
    }
}

/// Active training course.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Training {
    pub skill: Skill,
    pub weeks_left: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Staff {
    pub id: u32,
    pub name: String,
    pub programming: f32,
    pub design: f32,
    pub art: f32,
    pub audio: f32,
    pub speed: f32,
    /// 0..=100
    pub morale: f32,
    /// Weekly salary in dollars.
    pub salary: i64,
    pub is_founder: bool,
    /// Flavour trait id, e.g. `"rustacean"`.
    pub traits: Vec<String>,
    pub hired_week: u32,
    pub training: Option<Training>,
    /// Weeks worked on projects (drives slow experience gains).
    pub experience_weeks: u32,
}

impl Default for Staff {
    fn default() -> Self {
        Staff {
            id: 0,
            name: "Dev".into(),
            programming: 3.0,
            design: 3.0,
            art: 3.0,
            audio: 3.0,
            speed: 5.0,
            morale: 75.0,
            salary: 600,
            is_founder: false,
            traits: Vec::new(),
            hired_week: 0,
            training: None,
            experience_weeks: 0,
        }
    }
}

pub const MAX_SKILL: f32 = 10.0;
/// Programming level from which a developer counts as "senior".
pub const SENIOR_PROGRAMMING: f32 = 7.0;

impl Staff {
    /// The player's own character: decent programmer, weak elsewhere, works for free.
    pub fn founder(id: u32, name: &str) -> Staff {
        Staff {
            id,
            name: name.to_string(),
            programming: 5.0,
            design: 4.0,
            art: 3.0,
            audio: 3.0,
            speed: 5.0,
            morale: 85.0,
            salary: 0,
            is_founder: true,
            ..Staff::default()
        }
    }

    pub fn skill(&self, s: Skill) -> f32 {
        match s {
            Skill::Programming => self.programming,
            Skill::Design => self.design,
            Skill::Art => self.art,
            Skill::Audio => self.audio,
            Skill::Speed => self.speed,
        }
    }

    pub fn raise(&mut self, s: Skill, by: f32) {
        let v = (self.skill(s) + by).clamp(1.0, MAX_SKILL);
        match s {
            Skill::Programming => self.programming = v,
            Skill::Design => self.design = v,
            Skill::Art => self.art = v,
            Skill::Audio => self.audio = v,
            Skill::Speed => self.speed = v,
        }
    }

    /// How good this person is at one quality category (0..=10).
    pub fn category_skill(&self, c: Category) -> f32 {
        match c {
            Category::Gameplay => 0.55 * self.programming + 0.45 * self.design,
            Category::Graphics => 0.75 * self.art + 0.25 * self.programming,
            Category::Story => 0.8 * self.design + 0.2 * self.art,
            Category::Audio => 0.85 * self.audio + 0.15 * self.programming,
            Category::Performance => self.programming,
        }
    }

    /// Average of the four craft skills.
    pub fn level(&self) -> f32 {
        (self.programming + self.design + self.art + self.audio) / 4.0
    }

    pub fn morale_factor(&self) -> f32 {
        0.6 + 0.4 * (self.morale / 100.0).clamp(0.0, 1.0)
    }

    pub fn is_senior(&self) -> bool {
        self.programming >= SENIOR_PROGRAMMING
    }

    /// Work units produced per week (about 10 for an average developer).
    pub fn output(&self, crunch: bool, crunch_speed: f32) -> f32 {
        let base = 10.0 * (0.5 + self.speed / 10.0) * self.morale_factor();
        let base = if crunch { base * crunch_speed } else { base };
        if self.training.is_some() {
            base * 0.3
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_scales_with_speed_morale_crunch_and_training() {
        let mut s = Staff::default();
        let normal = s.output(false, 1.35);
        assert!(normal > 5.0 && normal < 20.0);
        assert!(s.output(true, 1.35) > normal);
        s.speed = 9.0;
        assert!(s.output(false, 1.35) > normal);
        s.speed = 5.0;
        s.morale = 10.0;
        assert!(s.output(false, 1.35) < normal);
        s.morale = 75.0;
        s.training = Some(Training { skill: Skill::Art, weeks_left: 3 });
        assert!(s.output(false, 1.35) < normal * 0.5);
    }

    #[test]
    fn skills_are_clamped_and_seniority_follows_programming() {
        let mut s = Staff::founder(1, "A");
        s.raise(Skill::Programming, 100.0);
        assert_eq!(s.programming, MAX_SKILL);
        assert!(s.is_senior());
        s.raise(Skill::Art, -100.0);
        assert_eq!(s.art, 1.0);
    }

    #[test]
    fn category_skills_use_the_right_stats() {
        let s = Staff { programming: 10.0, art: 1.0, design: 1.0, audio: 1.0, ..Staff::default() };
        assert!(s.category_skill(Category::Performance) > s.category_skill(Category::Graphics));
        let artist = Staff { art: 10.0, programming: 1.0, ..Staff::default() };
        assert!(artist.category_skill(Category::Graphics) > 7.0);
    }
}

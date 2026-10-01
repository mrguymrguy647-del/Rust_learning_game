//! Basic vocabulary shared by the whole simulation: quality categories, weights,
//! project sizes, audiences and development phases.

use serde::{Deserialize, Serialize};

/// The five quality dimensions of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Gameplay,
    Graphics,
    Story,
    Audio,
    Performance,
}

impl Category {
    pub const ALL: [Category; 5] =
        [Category::Gameplay, Category::Graphics, Category::Story, Category::Audio, Category::Performance];

    pub fn label(self) -> &'static str {
        match self {
            Category::Gameplay => "Gameplay",
            Category::Graphics => "Graphics",
            Category::Story => "Story",
            Category::Audio => "Audio",
            Category::Performance => "Performance",
        }
    }
}

/// One number per [`Category`]. Used for focus sliders, genre ideals, reviewer tastes and
/// engine bonuses.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Weights {
    pub gameplay: f32,
    pub graphics: f32,
    pub story: f32,
    pub audio: f32,
    pub performance: f32,
}

impl Weights {
    pub const fn new(gameplay: f32, graphics: f32, story: f32, audio: f32, performance: f32) -> Weights {
        Weights { gameplay, graphics, story, audio, performance }
    }

    /// Equal split totalling 100.
    pub const EVEN: Weights = Weights::new(20.0, 20.0, 20.0, 20.0, 20.0);

    pub fn get(&self, c: Category) -> f32 {
        match c {
            Category::Gameplay => self.gameplay,
            Category::Graphics => self.graphics,
            Category::Story => self.story,
            Category::Audio => self.audio,
            Category::Performance => self.performance,
        }
    }

    pub fn set(&mut self, c: Category, v: f32) {
        match c {
            Category::Gameplay => self.gameplay = v,
            Category::Graphics => self.graphics = v,
            Category::Story => self.story = v,
            Category::Audio => self.audio = v,
            Category::Performance => self.performance = v,
        }
    }

    pub fn sum(&self) -> f32 {
        Category::ALL.iter().map(|c| self.get(*c)).sum()
    }

    /// Scaled so the values sum to `total` (an all-zero input becomes an even split).
    pub fn normalized_to(&self, total: f32) -> Weights {
        let s = self.sum();
        if s <= 0.0 {
            return Weights::EVEN.normalized_to(total);
        }
        let mut out = *self;
        for c in Category::ALL {
            out.set(c, self.get(c) / s * total);
        }
        out
    }

    pub fn map(&self, f: impl Fn(Category, f32) -> f32) -> Weights {
        let mut out = *self;
        for c in Category::ALL {
            out.set(c, f(c, self.get(c)));
        }
        out
    }

    /// Change one slider and rebalance the others proportionally so the total stays `total`.
    pub fn with_adjusted(&self, changed: Category, new_value: f32, total: f32) -> Weights {
        let new_value = new_value.clamp(0.0, total);
        let others_sum: f32 = Category::ALL.iter().filter(|c| **c != changed).map(|c| self.get(*c)).sum();
        let remaining = total - new_value;
        let mut out = *self;
        out.set(changed, new_value);
        for c in Category::ALL.iter().filter(|c| **c != changed) {
            let v = if others_sum > 0.0 { self.get(*c) / others_sum * remaining } else { remaining / 4.0 };
            out.set(*c, v);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Audience {
    Everyone,
    Teen,
    Mature,
}

impl Audience {
    pub const ALL: [Audience; 3] = [Audience::Everyone, Audience::Teen, Audience::Mature];

    pub fn label(self) -> &'static str {
        match self {
            Audience::Everyone => "Everyone",
            Audience::Teen => "Teen",
            Audience::Mature => "Mature",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Audience::Everyone => 0,
            Audience::Teen => 1,
            Audience::Mature => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProjectSize {
    Small,
    Medium,
    Large,
    AAA,
}

impl ProjectSize {
    pub const ALL: [ProjectSize; 4] =
        [ProjectSize::Small, ProjectSize::Medium, ProjectSize::Large, ProjectSize::AAA];

    pub fn label(self) -> &'static str {
        match self {
            ProjectSize::Small => "Small",
            ProjectSize::Medium => "Medium",
            ProjectSize::Large => "Large",
            ProjectSize::AAA => "AAA",
        }
    }

    pub fn index(self) -> usize {
        match self {
            ProjectSize::Small => 0,
            ProjectSize::Medium => 1,
            ProjectSize::Large => 2,
            ProjectSize::AAA => 3,
        }
    }
}

/// Development phases. Challenges ("blockers") can appear in any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Phase {
    Concept,
    Core,
    Content,
    Polish,
}

impl Phase {
    pub const ALL: [Phase; 4] = [Phase::Concept, Phase::Core, Phase::Content, Phase::Polish];

    pub fn label(self) -> &'static str {
        match self {
            Phase::Concept => "Concept",
            Phase::Core => "Core systems",
            Phase::Content => "Content",
            Phase::Polish => "Polish",
        }
    }

    /// Fraction of the total work at which each phase ends.
    pub fn end_fraction(self) -> f32 {
        match self {
            Phase::Concept => 0.15,
            Phase::Core => 0.50,
            Phase::Content => 0.85,
            Phase::Polish => 1.0,
        }
    }

    pub fn for_fraction(f: f32) -> Phase {
        Phase::ALL.into_iter().find(|p| f < p.end_fraction()).unwrap_or(Phase::Polish)
    }

    /// Which categories this phase mostly advances (multiplier on the focus share).
    pub fn emphasis(self, c: Category) -> f32 {
        use Category::*;
        match (self, c) {
            (Phase::Concept, Gameplay | Story) => 1.5,
            (Phase::Concept, _) => 0.6,
            (Phase::Core, Gameplay | Performance) => 1.4,
            (Phase::Core, _) => 0.7,
            (Phase::Content, Graphics | Audio | Story) => 1.35,
            (Phase::Content, _) => 0.6,
            (Phase::Polish, _) => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusting_a_slider_keeps_the_total() {
        let w = Weights::EVEN;
        let out = w.with_adjusted(Category::Story, 60.0, 100.0);
        assert!((out.sum() - 100.0).abs() < 1e-3);
        assert_eq!(out.story, 60.0);
        assert!((out.gameplay - 10.0).abs() < 1e-3);
        let all_in = w.with_adjusted(Category::Audio, 100.0, 100.0);
        assert!((all_in.sum() - 100.0).abs() < 1e-3);
        assert_eq!(all_in.gameplay, 0.0);
    }

    #[test]
    fn zero_weights_normalize_to_even_split() {
        let n = Weights::default().normalized_to(100.0);
        assert!((n.gameplay - 20.0).abs() < 1e-4 && (n.sum() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn phases_partition_the_work() {
        assert_eq!(Phase::for_fraction(0.0), Phase::Concept);
        assert_eq!(Phase::for_fraction(0.3), Phase::Core);
        assert_eq!(Phase::for_fraction(0.6), Phase::Content);
        assert_eq!(Phase::for_fraction(0.99), Phase::Polish);
        assert_eq!(Phase::for_fraction(1.5), Phase::Polish);
        let mut last = 0.0;
        for p in Phase::ALL {
            assert!(p.end_fraction() > last);
            last = p.end_fraction();
        }
    }
}

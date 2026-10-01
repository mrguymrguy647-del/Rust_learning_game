//! Turning a finished project into quality numbers.
//!
//! Each category's score depends on the team's effective skill (craft skills × engine bonuses)
//! relative to what a game of that size is expected to reach. The focus sliders decide how much
//! each category weighs, and how well they match the genre's ideal split decides the genre fit.

use serde::{Deserialize, Serialize};

use super::library::ReleasedGame;
use super::model::{Category, Weights};
use super::project::Project;
use crate::data::ContentLibrary;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QualityReport {
    /// 0..=100 per category.
    pub categories: Weights,
    /// 0..=100 overall.
    pub overall: f32,
    /// 0..=1: how closely the focus split matches the genre's ideal.
    pub fit: f32,
    pub theme_factor: f32,
    pub novelty_factor: f32,
    pub bug_penalty: f32,
    pub challenge_bonus: f32,
    /// Estimated bug level in percent (0..=100).
    pub bugs: f32,
}

/// 0..=1 similarity between a focus split and the genre's ideal split.
pub fn focus_fit(focus: &Weights, ideal: &Weights) -> f32 {
    let (f, i) = (focus.normalized_to(100.0), ideal.normalized_to(100.0));
    let distance: f32 = Category::ALL.iter().map(|c| (f.get(*c) - i.get(*c)).abs()).sum();
    (1.0 - 0.5 * distance / 100.0).clamp(0.0, 1.0)
}

/// Estimated final bug percentage for a project.
pub fn estimate_bugs(content: &ContentLibrary, p: &Project) -> f32 {
    let prog_effective = p.avg_effective().get(Category::Performance);
    let size_base = 8.0 + 4.0 * p.size.index() as f32;
    let skill_factor = (1.5 - prog_effective * 1.4).clamp(0.35, 1.4);
    let _ = content;
    (size_base * skill_factor + p.crunch_bugs + 8.0 * p.skipped_blockers as f32).clamp(0.0, 100.0)
}

pub fn evaluate(content: &ContentLibrary, p: &Project, past: &[ReleasedGame]) -> QualityReport {
    let balance = &content.balance;
    let size = balance.size(p.size);
    let genre = content.genres.iter().find(|g| g.id == p.genre);
    let ideal = genre.map(|g| g.ideal).unwrap_or(Weights::EVEN);
    let focus = p.focus.normalized_to(100.0);
    let ideal_n = ideal.normalized_to(100.0);
    let eff = p.avg_effective();

    let mut categories = Weights::default();
    for c in Category::ALL {
        let ratio = (eff.get(c) / size.expectation).max(0.0);
        let boost = ((focus.get(c) + 5.0) / (ideal_n.get(c) + 5.0)).powf(0.35).clamp(0.75, 1.25);
        categories.set(c, (100.0 * ratio.powf(0.9) * boost).clamp(0.0, 100.0));
    }
    let base: f32 = Category::ALL.iter().map(|c| focus.get(*c) / 100.0 * categories.get(*c)).sum();

    let fit = focus_fit(&p.focus, &ideal);
    let theme_factor = genre.map(|g| g.theme_factor(&p.theme)).unwrap_or(1.0).clamp(0.6, 1.3);
    let recent: Vec<&ReleasedGame> = past.iter().rev().take(3).collect();
    let same_combo = recent.iter().filter(|g| g.genre == p.genre && g.theme == p.theme).count() as f32;
    let same_genre = recent.iter().filter(|g| g.genre == p.genre && g.theme != p.theme).count() as f32;
    let novelty_factor = (1.0 - 0.08 * same_combo - 0.03 * same_genre).clamp(0.7, 1.0);

    let bugs = estimate_bugs(content, p);
    let bug_penalty = (bugs / 100.0 * 0.8).min(0.5);
    let challenge_bonus = p.challenge_quality.min(20.0);

    let overall = (base * (0.75 + 0.25 * fit) * theme_factor * novelty_factor * (1.0 - bug_penalty)
        + challenge_bonus * 0.6)
        .clamp(0.0, 100.0);

    QualityReport {
        categories,
        overall,
        fit,
        theme_factor,
        novelty_factor,
        bug_penalty,
        challenge_bonus,
        bugs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_focus_has_fit_one_and_opposite_is_low() {
        let ideal = Weights::new(55.0, 10.0, 5.0, 15.0, 15.0);
        assert!((focus_fit(&ideal, &ideal) - 1.0).abs() < 1e-5);
        let opposite = Weights::new(0.0, 0.0, 100.0, 0.0, 0.0);
        assert!(focus_fit(&opposite, &ideal) < 0.5);
    }
}

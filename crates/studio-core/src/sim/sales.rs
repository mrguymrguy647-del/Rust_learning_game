//! Sales model: launch spike, then a decaying tail.

use super::model::{Audience, ProjectSize};
use super::rng::GameRng;
use crate::data::ContentLibrary;

pub struct DemandInput<'a> {
    pub genre: &'a str,
    pub platform: &'a str,
    pub audience: Audience,
    pub size: ProjectSize,
    /// 0..=100
    pub hype: f32,
    pub fans: i64,
    /// 0..=100
    pub metascore: f32,
    pub week: u32,
    /// Market multiplier for this genre right now (1.0 = neutral). Filled by the market module.
    pub trend: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Demand {
    pub launch_units: f32,
    pub decay: f32,
}

/// Weekly sales retention: good games keep selling for longer.
pub fn decay_for(content: &ContentLibrary, metascore: f32) -> f32 {
    let b = &content.balance;
    let t = (metascore / 100.0).clamp(0.0, 1.0);
    b.decay_low + (b.decay_high - b.decay_low) * t
}

/// How much a review score amplifies sales (≈0.3 at 40, ≈0.7 at 60, ≈1.35 at 80, ≈1.7 at 90).
pub fn review_multiplier(metascore: f32) -> f32 {
    ((metascore / 100.0).clamp(0.0, 1.0)).powf(2.2) * 2.2
}

pub fn fan_multiplier(fans: i64) -> f32 {
    1.0 + ((1 + fans.max(0)) as f32).log10() / 4.0
}

pub fn demand(content: &ContentLibrary, d: &DemandInput) -> Demand {
    let size = content.balance.size(d.size);
    let genre = content.genres.iter().find(|g| g.id == d.genre);
    let platform = content.platforms.iter().find(|p| p.id == d.platform);
    let genre_pop = genre.map(|g| g.popularity).unwrap_or(1.0);
    let audience_fit = genre.map(|g| g.audience_factor(d.audience.index())).unwrap_or(1.0);
    let market = platform.map(|p| p.market_at(d.week)).unwrap_or(1.0);
    let launch = size.base_demand
        * market
        * genre_pop
        * d.trend
        * audience_fit
        * (1.0 + d.hype / 100.0)
        * fan_multiplier(d.fans)
        * review_multiplier(d.metascore);
    Demand { launch_units: launch.max(0.0), decay: decay_for(content, d.metascore) }
}

/// Units sold in `week` by a game released at `release_week`.
pub fn weekly_units(
    content: &ContentLibrary,
    launch_units: f32,
    decay: f32,
    release_week: u32,
    platform: &str,
    week: u32,
    rng: &mut GameRng,
) -> u32 {
    let age = week.saturating_sub(release_week) as i32;
    let market_ratio = content
        .platforms
        .iter()
        .find(|p| p.id == platform)
        .map(|p| {
            let at_release = p.market_at(release_week).max(0.01);
            (p.market_at(week) / at_release).clamp(0.3, 2.5).sqrt()
        })
        .unwrap_or(1.0);
    let noise = 1.0 + rng.range_f32(-0.08, 0.08);
    let units = launch_units * decay.powi(age) * market_ratio * noise;
    if units < content.balance.min_weekly_units {
        0
    } else {
        units.round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_and_fan_multipliers_are_monotonic() {
        let mut prev = -1.0;
        for m in (0..=100).step_by(5) {
            let v = review_multiplier(m as f32);
            assert!(v >= prev);
            prev = v;
        }
        assert!(review_multiplier(90.0) > 2.0 * review_multiplier(60.0));
        assert!(fan_multiplier(10_000) > fan_multiplier(10));
        assert!((fan_multiplier(0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn better_reviews_mean_a_longer_tail() {
        let lib = ContentLibrary::load(None);
        assert!(decay_for(&lib, 90.0) > decay_for(&lib, 40.0));
    }

    #[test]
    fn sales_decay_over_time_and_eventually_stop() {
        let lib = ContentLibrary::load(None);
        let mut rng = GameRng::from_seed(1);
        let first = weekly_units(&lib, 2000.0, 0.7, 10, "pc", 10, &mut rng);
        let later = weekly_units(&lib, 2000.0, 0.7, 10, "pc", 14, &mut rng);
        let ancient = weekly_units(&lib, 2000.0, 0.7, 10, "pc", 80, &mut rng);
        assert!(first > later && later > 0);
        assert_eq!(ancient, 0);
    }
}

//! Released games: reviews, sales history and revenue.

use serde::{Deserialize, Serialize};

use super::model::{Audience, ProjectSize, Weights};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub outlet_id: String,
    pub outlet: String,
    /// 1.0..=10.0 in half-point steps.
    pub score: f32,
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleasedGame {
    pub id: u32,
    pub name: String,
    pub genre: String,
    pub theme: String,
    pub platform: String,
    pub audience: Audience,
    pub size: ProjectSize,
    pub release_week: u32,
    /// Category scores, 0..=100.
    pub categories: Weights,
    pub quality: f32,
    pub bugs: f32,
    pub reviews: Vec<Review>,
    /// 0..=100
    pub metascore: f32,
    pub hype: f32,
    /// Retail price per unit (already adjusted for platform).
    pub price: f32,
    /// Fraction of the price the platform keeps.
    pub store_cut: f32,
    pub launch_units: f32,
    /// Weekly sales decay factor.
    pub decay: f32,
    /// Units sold per week since release.
    pub sales: Vec<u32>,
    pub units_total: u64,
    pub revenue_total: i64,
    /// Money spent on development (salaries, rent, budget) — for profit reporting.
    pub dev_cost: i64,
    pub on_sale: bool,
    pub patches: u32,
    pub sequel_of: Option<u32>,
    pub fans_acc: f32,
    /// DLC entries have no reviews of their own and point at the base game.
    pub is_dlc: bool,
    pub parent: Option<u32>,
    pub dlc_count: u32,
    /// Genre/theme trend at release (sales follow the trend relative to this).
    pub trend_at_release: f32,
    /// Temporary sales boost (patch, hotfix, award): remaining weeks and multiplier.
    pub boost_weeks: u32,
    pub boost_mult: f32,
}

impl Default for ReleasedGame {
    fn default() -> Self {
        ReleasedGame {
            id: 0,
            name: String::new(),
            genre: String::new(),
            theme: String::new(),
            platform: "pc".into(),
            audience: Audience::Everyone,
            size: ProjectSize::Small,
            release_week: 0,
            categories: Weights::default(),
            quality: 0.0,
            bugs: 0.0,
            reviews: Vec::new(),
            metascore: 0.0,
            hype: 0.0,
            price: 0.0,
            store_cut: 0.25,
            launch_units: 0.0,
            decay: 0.7,
            sales: Vec::new(),
            units_total: 0,
            revenue_total: 0,
            dev_cost: 0,
            on_sale: true,
            patches: 0,
            sequel_of: None,
            fans_acc: 0.0,
            is_dlc: false,
            parent: None,
            dlc_count: 0,
            trend_at_release: 1.0,
            boost_weeks: 0,
            boost_mult: 1.0,
        }
    }
}

impl ReleasedGame {
    pub fn age(&self, week: u32) -> u32 {
        week.saturating_sub(self.release_week)
    }

    pub fn profit(&self) -> i64 {
        self.revenue_total - self.dev_cost
    }

    pub fn weekly_units_last(&self) -> u32 {
        self.sales.last().copied().unwrap_or(0)
    }
}

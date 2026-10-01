//! Money: weekly income and expenses, the finance history and bankruptcy.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::notify::NoteKind;
use super::state::GameState;
use crate::data::ContentLibrary;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WeekFinance {
    pub week: u32,
    pub revenue: i64,
    pub rent: i64,
    pub salaries: i64,
    pub dev: i64,
    pub other: i64,
}

impl WeekFinance {
    pub fn expenses(&self) -> i64 {
        self.rent + self.salaries + self.dev + self.other
    }

    pub fn net(&self) -> i64 {
        self.revenue - self.expenses()
    }
}

pub const FINANCE_HISTORY_WEEKS: usize = 104;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FinanceBook {
    pub weekly: VecDeque<WeekFinance>,
    pub lifetime_revenue: i64,
    pub lifetime_expenses: i64,
}

impl FinanceBook {
    pub fn record(&mut self, w: WeekFinance) {
        self.lifetime_revenue += w.revenue;
        self.lifetime_expenses += w.expenses();
        self.weekly.push_back(w);
        while self.weekly.len() > FINANCE_HISTORY_WEEKS {
            self.weekly.pop_front();
        }
    }

    /// Average weekly net over the last `weeks` recorded weeks.
    pub fn average_net(&self, weeks: usize) -> i64 {
        let recent: Vec<_> = self.weekly.iter().rev().take(weeks).collect();
        if recent.is_empty() {
            0
        } else {
            recent.iter().map(|w| w.net()).sum::<i64>() / recent.len() as i64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameOver {
    pub week: u32,
    pub reason: String,
}

impl GameState {
    /// Weekly rent and salaries (already scaled by difficulty).
    pub fn weekly_fixed_costs(&self, content: &ContentLibrary) -> (i64, i64) {
        let mult = self.difficulty.cost_mult();
        let rent = content.tier(self.studio.tier).map(|t| t.rent).unwrap_or(0);
        let salaries: i64 = self.staff.iter().map(|s| s.salary).sum();
        ((rent as f32 * mult).round() as i64, (salaries as f32 * mult).round() as i64)
    }

    /// Weeks of cash left at the current burn rate (rent + salaries), `None` if there is no burn.
    pub fn runway_weeks(&self, content: &ContentLibrary) -> Option<i64> {
        let (rent, salaries) = self.weekly_fixed_costs(content);
        let burn = rent + salaries;
        if burn <= 0 {
            None
        } else {
            Some((self.studio.money / burn).max(0))
        }
    }

    /// Track consecutive weeks in the red; warn, then close the studio.
    pub(crate) fn check_bankruptcy(&mut self, content: &ContentLibrary) {
        let limit = content.balance.bankruptcy_weeks.max(1);
        if self.studio.money >= 0 {
            if self.debt_weeks > 0 {
                self.note(NoteKind::Good, "You are back in the black. Phew.");
            }
            self.debt_weeks = 0;
            return;
        }
        self.debt_weeks += 1;
        let left = limit.saturating_sub(self.debt_weeks);
        if self.debt_weeks >= limit {
            let week = self.date.week();
            self.game_over = Some(GameOver {
                week,
                reason: format!("{} ran out of money and could not pay its bills.", self.studio.name),
            });
            self.note(NoteKind::Bad, "The studio has gone bankrupt.");
        } else if self.debt_weeks == 1 || left <= 3 {
            self.note(
                NoteKind::Bad,
                format!("Bankruptcy warning: you are in debt. {left} week(s) left to get back above zero."),
            );
        }
    }
}

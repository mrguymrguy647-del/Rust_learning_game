//! Game calendar: one tick = one week, 52 weeks per year.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const WEEKS_PER_YEAR: u32 = 52;

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Absolute week counter (0 = first week of year 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct GameDate(pub u32);

impl GameDate {
    pub fn week(self) -> u32 {
        self.0
    }

    /// 1-based year.
    pub fn year(self) -> u32 {
        self.0 / WEEKS_PER_YEAR + 1
    }

    /// 1-based week within the year.
    pub fn week_of_year(self) -> u32 {
        self.0 % WEEKS_PER_YEAR + 1
    }

    /// 0-based month index.
    pub fn month_index(self) -> usize {
        ((self.0 % WEEKS_PER_YEAR) * 12 / WEEKS_PER_YEAR) as usize
    }

    pub fn month_name(self) -> &'static str {
        MONTHS[self.month_index().min(11)]
    }

    /// True on the first week of each month (used for payroll-style monthly events).
    pub fn is_month_start(self) -> bool {
        let w = self.0 % WEEKS_PER_YEAR;
        w == 0 || (w * 12 / WEEKS_PER_YEAR) != ((w - 1) * 12 / WEEKS_PER_YEAR)
    }

    pub fn advance(&mut self, weeks: u32) {
        self.0 = self.0.saturating_add(weeks);
    }
}

impl fmt::Display for GameDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Year {} · {} · Week {}", self.year(), self.month_name(), self.week_of_year())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_math() {
        let d = GameDate(0);
        assert_eq!((d.year(), d.week_of_year(), d.month_name()), (1, 1, "Jan"));
        let d = GameDate(52);
        assert_eq!((d.year(), d.week_of_year()), (2, 1));
        let d = GameDate(51);
        assert_eq!((d.year(), d.month_name()), (1, "Dec"));
    }

    #[test]
    fn twelve_month_starts_per_year() {
        let starts = (0..52).filter(|w| GameDate(*w).is_month_start()).count();
        assert_eq!(starts, 12);
    }
}

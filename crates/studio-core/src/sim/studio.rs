//! The studio itself: moving to a bigger office and borrowing money.

use serde::{Deserialize, Serialize};

use super::notify::NoteKind;
use super::state::GameState;
use crate::data::ContentLibrary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loan {
    pub id: u32,
    pub principal: i64,
    pub taken_week: u32,
}

/// What stands between the player and the next studio tier.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeStatus {
    pub next_tier: usize,
    pub name: String,
    pub cost: i64,
    pub required_reputation: i64,
    pub have_reputation: i64,
    pub problems: Vec<String>,
}

impl GameState {
    /// `None` when already at the top tier.
    pub fn upgrade_status(&self, content: &ContentLibrary) -> Option<UpgradeStatus> {
        let next = self.studio.tier + 1;
        let t = content.tier(next)?;
        let mut problems = Vec::new();
        if self.studio.reputation < t.required_reputation {
            problems.push(format!(
                "Needs {} fans (you have {}).",
                crate::fmt::compact(t.required_reputation),
                crate::fmt::compact(self.studio.reputation)
            ));
        }
        if self.studio.money < t.upgrade_cost {
            problems.push(format!(
                "Moving costs {} (you have {}).",
                crate::fmt::money(t.upgrade_cost),
                crate::fmt::money(self.studio.money)
            ));
        }
        if self.project.is_some() {
            problems.push("Finish or cancel the current project first (moving would disrupt it).".into());
        }
        Some(UpgradeStatus {
            next_tier: next,
            name: t.name.clone(),
            cost: t.upgrade_cost,
            required_reputation: t.required_reputation,
            have_reputation: self.studio.reputation,
            problems,
        })
    }

    pub fn upgrade_studio(&mut self, content: &ContentLibrary) -> Result<(), String> {
        let status = self.upgrade_status(content).ok_or("You already run the biggest studio there is.")?;
        if let Some(p) = status.problems.first() {
            return Err(p.clone());
        }
        self.studio.money -= status.cost;
        self.studio.tier = status.next_tier;
        self.progress.set_max("studio_tier", status.next_tier as i64);
        self.note(NoteKind::Good, format!("The studio moved into the {}!", status.name));
        self.refresh_candidates(content);
        Ok(())
    }

    // ---- loans -------------------------------------------------------------------------------

    pub fn loan_outstanding(&self) -> i64 {
        self.studio.loans.iter().map(|l| l.principal).sum()
    }

    pub fn loan_available(&self, content: &ContentLibrary) -> i64 {
        let limit = content.tier(self.studio.tier).map(|t| t.loan_limit).unwrap_or(0);
        (limit - self.loan_outstanding()).max(0)
    }

    pub fn weekly_interest(&self, content: &ContentLibrary) -> i64 {
        (self.loan_outstanding() as f32 * content.balance.loan_weekly_rate).round() as i64
    }

    pub fn take_loan(&mut self, content: &ContentLibrary, amount: i64) -> Result<(), String> {
        if amount <= 0 {
            return Err("Choose an amount.".into());
        }
        if amount > self.loan_available(content) {
            return Err(format!(
                "The bank will lend you at most {} right now.",
                crate::fmt::money(self.loan_available(content))
            ));
        }
        let id = self.next_id();
        self.studio.loans.push(Loan { id, principal: amount, taken_week: self.date.week() });
        self.studio.money += amount;
        self.note(NoteKind::Info, format!("You borrowed {}.", crate::fmt::money(amount)));
        Ok(())
    }

    /// Pay back up to `amount`, oldest loans first.
    pub fn repay_loan(&mut self, amount: i64) -> Result<i64, String> {
        if amount <= 0 || self.studio.loans.is_empty() {
            return Err("Nothing to repay.".into());
        }
        let amount = amount.min(self.loan_outstanding()).min(self.studio.money.max(0));
        if amount <= 0 {
            return Err("You have no cash to repay with.".into());
        }
        let mut left = amount;
        for loan in self.studio.loans.iter_mut() {
            let pay = left.min(loan.principal);
            loan.principal -= pay;
            left -= pay;
            if left == 0 {
                break;
            }
        }
        self.studio.loans.retain(|l| l.principal > 0);
        self.studio.money -= amount;
        if self.studio.loans.is_empty() {
            self.note(NoteKind::Good, "All loans repaid.");
        }
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        (lib.clone(), GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(2)))
    }

    #[test]
    fn upgrade_needs_fans_and_money_and_applies_new_tier() {
        let (lib, mut st) = setup();
        let status = st.upgrade_status(&lib).unwrap();
        assert!(status.problems.iter().any(|p| p.contains("fans")));
        assert!(st.upgrade_studio(&lib).is_err());
        st.studio.reputation = 10_000;
        st.studio.money = 1_000_000;
        let cost = status.cost;
        st.upgrade_studio(&lib).unwrap();
        assert_eq!(st.studio.tier, 1);
        assert_eq!(st.studio.money, 1_000_000 - cost);
        assert_eq!(st.staff_slots(&lib), lib.tier(1).unwrap().staff_slots);
        assert!(st.upgrade_status(&lib).unwrap().next_tier == 2);
    }

    #[test]
    fn top_tier_has_no_upgrade() {
        let (lib, mut st) = setup();
        st.studio.tier = lib.tiers.len() - 1;
        assert!(st.upgrade_status(&lib).is_none());
        assert!(st.upgrade_studio(&lib).is_err());
    }

    #[test]
    fn loans_respect_the_limit_accrue_interest_and_can_be_repaid() {
        let (lib, mut st) = setup();
        let limit = lib.tier(0).unwrap().loan_limit;
        assert!(st.take_loan(&lib, limit + 1).is_err());
        st.take_loan(&lib, limit).unwrap();
        assert_eq!(st.loan_available(&lib), 0);
        assert!(st.take_loan(&lib, 1).is_err());
        let interest = st.weekly_interest(&lib);
        assert!(interest > 0);
        let before = st.studio.money;
        st.advance_week(&lib);
        let rent = lib.tier(0).unwrap().rent;
        assert_eq!(before - st.studio.money, rent + interest, "interest is paid every week");
        let paid = st.repay_loan(1000).unwrap();
        assert_eq!(paid, 1000);
        assert_eq!(st.loan_outstanding(), limit - 1000);
        st.studio.money = 1_000_000;
        st.repay_loan(i64::MAX).unwrap();
        assert!(st.studio.loans.is_empty());
    }
}

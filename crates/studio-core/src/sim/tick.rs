//! The weekly simulation step.

use super::economy::WeekFinance;
use super::notify::NoteKind;
use super::sales;
use super::state::GameState;
use crate::data::ContentLibrary;

/// Morale the team drifts back to when nobody is crunching.
const MORALE_BASELINE: f32 = 75.0;

impl GameState {
    /// Advance one week. Does nothing while a challenge is pending or after game over.
    pub fn advance_week(&mut self, content: &ContentLibrary) {
        self.tick(content, false);
    }

    /// Let `weeks` pass without triggering new challenges or events (used when the player pays a
    /// hint with time while a challenge is open).
    pub fn advance_weeks_quiet(&mut self, content: &ContentLibrary, weeks: u32) {
        for _ in 0..weeks {
            self.tick(content, true);
        }
    }

    fn tick(&mut self, content: &ContentLibrary, quiet: bool) {
        if self.game_over.is_some() || (!quiet && self.pending_attempt.is_some()) {
            return;
        }
        self.date.advance(1);
        let week = self.date.week();
        let mut fin = WeekFinance { week, ..WeekFinance::default() };

        self.process_sales(content, &mut fin);

        let (rent, salaries) = self.weekly_fixed_costs(content);
        fin.rent = rent;
        fin.salaries = salaries;

        let interest = self.weekly_interest(content);
        fin.other += interest;

        self.advance_project(content, &mut fin, quiet);
        self.update_morale(content);
        self.advance_engine(content);
        self.advance_staff(content);

        self.studio.money += fin.net();
        if let Some(p) = self.project.as_mut() {
            p.cost_so_far += fin.rent + fin.salaries + fin.dev;
        }
        self.finances.record(fin);
        self.check_bankruptcy(content);
    }

    fn process_sales(&mut self, content: &ContentLibrary, fin: &mut WeekFinance) {
        let week = self.date.week();
        let rev_mult = self.difficulty.revenue_mult();
        for i in 0..self.games.len() {
            if !self.games[i].on_sale {
                continue;
            }
            let (launch, decay, release, platform) = {
                let g = &self.games[i];
                (g.launch_units, g.decay, g.release_week, g.platform.clone())
            };
            // The first tick after release is the launch week (age 0).
            let units =
                sales::weekly_units(content, launch, decay, release, &platform, week - 1, &mut self.rng);
            let g = &mut self.games[i];
            let revenue = (units as f32 * g.price * (1.0 - g.store_cut) * rev_mult).round() as i64;
            g.sales.push(units);
            g.units_total += units as u64;
            g.revenue_total += revenue;
            g.fans_acc += units as f32 / 60.0;
            let whole = g.fans_acc.floor();
            g.fans_acc -= whole;
            fin.revenue += revenue;
            self.studio.reputation += whole as i64;
            let finished = units == 0 && g.age(week) >= 4;
            if finished {
                g.on_sale = false;
                let text = format!(
                    "“{}” has run its course: {} units, {} revenue.",
                    g.name,
                    g.units_total,
                    crate::fmt::money(g.revenue_total)
                );
                self.note(NoteKind::Info, text);
            }
        }
    }

    fn advance_project(&mut self, content: &ContentLibrary, fin: &mut WeekFinance, quiet: bool) {
        let Some(crunch) = self.project.as_ref().map(|p| p.crunch) else {
            return;
        };
        let complete_before = self.project.as_ref().is_some_and(|p| p.is_complete());
        if !complete_before {
            let team = self.team_week(content, crunch);
            let balance = &content.balance;
            if let Some(p) = self.project.as_mut() {
                let work = team.output.min(p.work_total - p.work_done).max(0.0);
                p.work_done += work;
                p.output_acc += work;
                p.bug_mult_acc += team.bug_mult * work;
                p.effective_acc = p.effective_acc.map(|c, v| v + team.effective.get(c) * work);
                p.weeks_in_dev += 1;
                let budget_part = if p.work_total > 0.0 {
                    (p.budget_total as f32 * work / p.work_total).round() as i64
                } else {
                    0
                };
                p.budget_spent += budget_part;
                fin.dev += budget_part;
                if p.crunch {
                    p.crunch_weeks += 1;
                    p.crunch_bugs += balance.crunch_bugs;
                }
                // Hype fades slowly while the game is in development.
                p.hype = (p.hype - 0.15).max(0.0);
            }
            for s in self.staff.iter_mut() {
                s.experience_weeks += 1;
            }
        }

        let complete_now = self.project.as_ref().is_some_and(|p| p.is_complete());
        if !quiet {
            if let Some(i) = self.project.as_ref().and_then(|p| p.due_blocker()) {
                if self.pending_attempt.is_none() {
                    self.trigger_blocker(content, i);
                }
            }
            if complete_now && !complete_before {
                if let Some(name) = self.project.as_ref().map(|p| p.name.clone()) {
                    self.note(NoteKind::Good, format!("“{name}” is finished and ready to release!"));
                }
            }
        }
    }

    fn update_morale(&mut self, content: &ContentLibrary) {
        let crunching = self.project.as_ref().is_some_and(|p| p.crunch && !p.is_complete());
        let loss = content.balance.crunch_morale_loss;
        for s in self.staff.iter_mut() {
            if crunching {
                s.morale = (s.morale - loss).max(0.0);
            } else if s.morale < MORALE_BASELINE {
                s.morale = (s.morale + 2.0).min(MORALE_BASELINE);
            }
        }
    }
}

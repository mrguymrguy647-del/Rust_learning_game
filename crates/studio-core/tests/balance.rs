//! Balance checks: scripted careers (see `sim::autoplay`) must feel fair.
//!
//! A bot plays whole studio careers (8 game years) on fixed seeds. The assertions are deliberately
//! loose bands, not exact numbers: they catch "everyone is a billionaire by year three" and "nobody
//! can ever make rent" regressions when content or formulas are tuned. Use the `#[ignore]`d probes
//! at the bottom (`cargo test -p studio-core --test balance -- --ignored --nocapture`) to look at
//! the actual numbers while tuning.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use studio_core::data::ContentLibrary;
use studio_core::settings::Difficulty;
use studio_core::sim::autoplay::{play_career, BotProfile, CareerReport};

const YEARS: u32 = 8;
const SEEDS: u64 = 12;

fn careers(profile: BotProfile, difficulty: Difficulty) -> Vec<CareerReport> {
    let lib = ContentLibrary::embedded();
    (1..=SEEDS).map(|seed| play_career(&lib, seed, difficulty, &profile, 52 * YEARS)).collect()
}

fn median(mut values: Vec<i64>) -> i64 {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}

fn survivors(reports: &[CareerReport]) -> usize {
    reports.iter().filter(|r| r.survived()).count()
}

fn median_final_cash(reports: &[CareerReport]) -> i64 {
    median(reports.iter().map(|r| r.final_money).collect())
}

fn median_cash_at_year(reports: &[CareerReport], year: usize) -> i64 {
    median(reports.iter().map(|r| r.money_by_year.get(year - 1).copied().unwrap_or(-1)).collect())
}

fn median_tier_at_year(reports: &[CareerReport], year: usize) -> i64 {
    median(reports.iter().map(|r| r.tier_by_year.get(year - 1).copied().unwrap_or(0) as i64).collect())
}

#[test]
fn a_competent_player_builds_a_studio_without_going_broke() {
    let r = careers(BotProfile::COMPETENT, Difficulty::Normal);
    assert!(survivors(&r) >= 11, "only {}/{SEEDS} competent players survived", survivors(&r));

    // First game out within the first year, and it earns something.
    let first = median(r.iter().filter_map(|c| c.first_release_week.map(i64::from)).collect());
    assert!((10..=45).contains(&first), "median first release in week {first}");
    let year1 = median_cash_at_year(&r, 1);
    assert!((15_000..=250_000).contains(&year1), "cash after year 1: {year1}");

    // The ladder: a garage by the end of year 3, a studio floor by the end of year 8.
    assert!(median_tier_at_year(&r, 3) >= 1, "no garage after three years");
    assert!(median_tier_at_year(&r, 8) >= 2, "still tiny after eight years");

    // Money grows but never explodes.
    let final_cash = median_final_cash(&r);
    assert!((1_000_000..=100_000_000).contains(&final_cash), "cash after 8 years: {final_cash}");
    assert!(
        median_cash_at_year(&r, 4) < 15_000_000,
        "year-4 cash {} suggests runaway profits",
        median_cash_at_year(&r, 4)
    );

    // Reviews land in a believable band.
    let meta = median(r.iter().map(|c| c.average_meta as i64).collect());
    assert!((50..=85).contains(&meta), "median average metascore {meta}");
}

#[test]
fn beginners_can_survive_and_hard_mode_bites() {
    let novice_normal = careers(BotProfile::NOVICE, Difficulty::Normal);
    assert!(survivors(&novice_normal) >= 9, "novices go bankrupt too often on Normal");
    let novice_hard = careers(BotProfile::NOVICE, Difficulty::Hard);
    assert!(survivors(&novice_hard) >= 7, "Hard is unwinnable for novices");
    assert!(survivors(&novice_hard) <= survivors(&novice_normal), "Hard should never be easier than Normal");
}

#[test]
fn difficulty_scales_the_economy_in_order() {
    for profile in [BotProfile::NOVICE, BotProfile::COMPETENT] {
        let easy = median_final_cash(&careers(profile, Difficulty::Easy));
        let normal = median_final_cash(&careers(profile, Difficulty::Normal));
        let hard = median_final_cash(&careers(profile, Difficulty::Hard));
        assert!(easy > normal && normal > hard, "{}: easy {easy} normal {normal} hard {hard}", profile.name);
    }
}

#[test]
fn rust_skill_pays_off() {
    let novice = careers(BotProfile::NOVICE, Difficulty::Normal);
    let expert = careers(BotProfile::EXPERT, Difficulty::Normal);
    // Better programmers hire fewer contractors, learn more and earn more.
    let contractors = |r: &[CareerReport]| r.iter().map(|c| c.contractors_hired).sum::<i64>();
    assert!(contractors(&expert) < contractors(&novice), "experts should rely on contractors less");
    let solved = |r: &[CareerReport]| median(r.iter().map(|c| c.challenges_solved as i64).collect());
    assert!(solved(&expert) > solved(&novice), "experts should learn more");
    assert!(median_final_cash(&expert) > median_final_cash(&novice), "skill should be rewarded");
}

#[test]
fn careers_are_deterministic() {
    let lib = ContentLibrary::embedded();
    let a = play_career(&lib, 7, Difficulty::Normal, &BotProfile::COMPETENT, 52 * 3);
    let b = play_career(&lib, 7, Difficulty::Normal, &BotProfile::COMPETENT, 52 * 3);
    assert_eq!(a, b);
    let c = play_career(&lib, 8, Difficulty::Normal, &BotProfile::COMPETENT, 52 * 3);
    assert_ne!(a.money_by_year, c.money_by_year, "different seeds should play out differently");
}

// ---------------------------------------------------------------------------------------------
// Probes for tuning (ignored: they only print).

fn print(label: &str, r: &CareerReport) {
    println!(
        "{label:<22} {} wk{:>4} tier {} games {:>2} meta avg {:>4.0} best {:>3.0} rev {:>10} money {:>10} low {:>9} peak {:>10} solved {:>2} contr {} lvl {} mods {:>2} fans {:>7} first rel {:?} years {:?}",
        if r.survived() { "ok  " } else { "BANK" },
        r.weeks_played, r.tier, r.games_released, r.average_meta, r.best_meta, r.lifetime_revenue,
        r.final_money, r.lowest_money, r.peak_money, r.challenges_solved, r.contractors_hired, r.level,
        r.modules_built, r.reputation, r.first_release_week, r.money_by_year
    );
}

#[test]
#[ignore = "prints career summaries for tuning"]
fn probe() {
    let lib = ContentLibrary::embedded();
    for profile in [BotProfile::NOVICE, BotProfile::COMPETENT, BotProfile::EXPERT] {
        for diff in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            for seed in 1..=6u64 {
                let r = play_career(&lib, seed, diff, &profile, 52 * 8);
                print(&format!("{} {:?} s{seed}", profile.name, diff), &r);
            }
        }
    }
}

#[test]
#[ignore = "prints every release of one career"]
fn probe_games() {
    use studio_core::sim::autoplay::play_career_with_state;
    let lib = ContentLibrary::embedded();
    let seed: u64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let (r, st) = play_career_with_state(&lib, seed, Difficulty::Normal, &BotProfile::COMPETENT, 52 * 6);
    print("career", &r);
    for g in &st.games {
        println!(
            "wk {:>3} {:?} {:<12} {:<9} {:<10} q {:>3.0} meta {:>3.0} launch {:>8.0} units {:>9} rev {:>10} cost {:>10} price {:>5.2} fans {}",
            g.release_week, g.size, g.genre, g.platform, g.theme, g.quality, g.metascore, g.launch_units,
            g.units_total, g.revenue_total, g.dev_cost, g.price, 0
        );
    }
}

#[test]
#[ignore = "prints aggregate balance numbers"]
fn probe_summary() {
    let lib = ContentLibrary::embedded();
    let seeds: Vec<u64> = (1..=16).collect();
    for profile in [BotProfile::NOVICE, BotProfile::COMPETENT, BotProfile::EXPERT] {
        for diff in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            let reports: Vec<CareerReport> =
                seeds.iter().map(|&s| play_career(&lib, s, diff, &profile, 52 * 8)).collect();
            let alive = reports.iter().filter(|r| r.survived()).count();
            let years: Vec<i64> = (0..8)
                .map(|y| {
                    median(reports.iter().map(|r| r.money_by_year.get(y).copied().unwrap_or(-1)).collect())
                })
                .collect();
            let tiers: Vec<usize> = (0..5).map(|t| reports.iter().filter(|r| r.tier == t).count()).collect();
            let tier_years: Vec<i64> = (0..8)
                .map(|y| {
                    median(
                        reports.iter().map(|r| r.tier_by_year.get(y).copied().unwrap_or(0) as i64).collect(),
                    )
                })
                .collect();
            let meta = median(reports.iter().map(|r| r.average_meta as i64).collect());
            let games = median(reports.iter().map(|r| r.games_released as i64).collect());
            let first = median(reports.iter().filter_map(|r| r.first_release_week.map(i64::from)).collect());
            println!(
                "{:<9} {:<6} alive {:>2}/{} tiers {:?} by year {:?} meta {meta} games {games} first {first} cash {:?}",
                profile.name, format!("{diff:?}"), alive, reports.len(), tiers, tier_years, years
            );
        }
    }
}

#[test]
#[ignore = "long careers: is the top tier reachable?"]
fn probe_long() {
    let lib = ContentLibrary::embedded();
    for profile in [BotProfile::COMPETENT, BotProfile::EXPERT] {
        let reports: Vec<CareerReport> =
            (1..=10u64).map(|s| play_career(&lib, s, Difficulty::Normal, &profile, 52 * 14)).collect();
        for r in &reports {
            println!(
                "{} alive {} tiers {:?} cash {:?}",
                profile.name,
                r.survived(),
                r.tier_by_year,
                r.money_by_year.iter().map(|m| m / 1000).collect::<Vec<_>>()
            );
        }
    }
}

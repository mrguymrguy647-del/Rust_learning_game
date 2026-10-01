//! Simulation tests against the built-in content.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use studio_core::data::ContentLibrary;
use studio_core::save;
use studio_core::settings::Difficulty;
use studio_core::sim::{
    Attempt, Audience, ChallengeContext, GameState, Phase, ProjectConfig, ProjectSize, Weights,
};

fn content() -> ContentLibrary {
    ContentLibrary::embedded()
}

fn new_state(lib: &ContentLibrary) -> GameState {
    GameState::new_game(lib, "Test Studio", "Tester", Difficulty::Normal, Some(42))
}

fn puzzle_config(lib: &ContentLibrary) -> ProjectConfig {
    let genre = lib.genres.iter().find(|g| g.id == "puzzle").unwrap();
    ProjectConfig {
        name: "Block Party".into(),
        genre: "puzzle".into(),
        theme: "cooking".into(),
        platform: "pc".into(),
        audience: Audience::Everyone,
        size: ProjectSize::Small,
        focus: genre.ideal,
        sequel_of: None,
    }
}

/// Run weeks, solving every blocking challenge with a clean first-try solve.
fn run_until(lib: &ContentLibrary, st: &mut GameState, max_weeks: u32, done: impl Fn(&GameState) -> bool) {
    for _ in 0..max_weeks {
        if done(st) || st.game_over.is_some() {
            return;
        }
        if let Some(attempt) = st.pending_attempt.clone() {
            solve(lib, st, &attempt);
            continue;
        }
        st.advance_week(lib);
    }
}

fn solve(lib: &ContentLibrary, st: &mut GameState, attempt: &Attempt) {
    let c = lib.challenge(&attempt.challenge_id).expect("pending challenge exists").clone();
    let mut a = attempt.clone();
    a.submissions = 1;
    st.complete_challenge(lib, &c, &a);
    st.pending_attempt = None;
}

#[test]
fn builtin_balance_loads_with_all_four_sizes() {
    let lib = content();
    for s in ProjectSize::ALL {
        let d = lib.balance.size(s);
        assert_eq!(d.size, s);
        assert!(d.work > 0.0 && d.price > 0.0 && d.base_demand > 0.0);
    }
    assert!(lib.genres.len() >= 10 && lib.themes.len() >= 12 && lib.outlets.len() >= 5);
    for g in &lib.genres {
        assert!((g.ideal.sum() - 100.0).abs() < 0.5, "{} ideal must sum to 100", g.id);
        for (theme, _) in &g.theme_fit {
            assert!(lib.themes.iter().any(|t| &t.id == theme), "{}: unknown theme {theme}", g.id);
        }
    }
    for o in &lib.outlets {
        assert!((o.weights.sum() - 1.0).abs() < 0.01, "{} weights must sum to 1", o.id);
        assert!(
            !o.quotes_great.is_empty()
                && !o.quotes_good.is_empty()
                && !o.quotes_mixed.is_empty()
                && !o.quotes_bad.is_empty()
        );
    }
}

#[test]
fn idle_weeks_cost_rent_and_nothing_else() {
    let lib = content();
    let mut st = new_state(&lib);
    let start = st.studio.money;
    for _ in 0..10 {
        st.advance_week(&lib);
    }
    let rent = lib.tier(0).unwrap().rent;
    assert_eq!(st.date.week(), 10);
    assert_eq!(st.studio.money, start - 10 * rent);
    assert_eq!(st.finances.weekly.len(), 10);
}

#[test]
fn difficulty_scales_costs() {
    let lib = content();
    let cost = |d| {
        let mut st = GameState::new_game(&lib, "S", "F", d, Some(1));
        st.advance_week(&lib);
        lib.balance.start_money - st.studio.money
    };
    assert!(cost(Difficulty::Easy) < cost(Difficulty::Normal));
    assert!(cost(Difficulty::Normal) < cost(Difficulty::Hard));
}

#[test]
fn simulation_is_deterministic_for_a_seed() {
    let lib = content();
    let play = || {
        let mut st = new_state(&lib);
        st.start_project(&lib, puzzle_config(&lib)).unwrap();
        run_until(&lib, &mut st, 80, |s| s.project.as_ref().is_some_and(|p| p.is_complete()));
        st.release_project(&lib).unwrap();
        run_until(&lib, &mut st, 30, |_| false);
        st
    };
    assert_eq!(play(), play());
}

#[test]
fn project_validation_explains_what_is_missing() {
    let lib = content();
    let mut st = new_state(&lib);
    let mut cfg = puzzle_config(&lib);
    cfg.size = ProjectSize::Medium;
    assert!(st.project_start_problem(&lib, &cfg).unwrap().contains("Garage"));
    cfg.size = ProjectSize::Small;
    cfg.focus = Weights::new(10.0, 10.0, 10.0, 10.0, 10.0);
    assert!(st.project_start_problem(&lib, &cfg).unwrap().contains("100"));
    cfg.focus = Weights::EVEN;
    cfg.genre = "nope".into();
    assert!(st.project_start_problem(&lib, &cfg).is_some());
    cfg = puzzle_config(&lib);
    st.start_project(&lib, cfg.clone()).unwrap();
    assert!(st.project_start_problem(&lib, &cfg).unwrap().contains("already"));
}

#[test]
fn a_small_game_goes_from_idea_to_release_with_blockers() {
    let lib = content();
    let mut st = new_state(&lib);
    st.start_project(&lib, puzzle_config(&lib)).unwrap();

    let mut saw_blocker = false;
    let mut phases = Vec::new();
    for _ in 0..120 {
        if let Some(a) = st.pending_attempt.clone() {
            saw_blocker = true;
            assert_eq!(a.context, ChallengeContext::Project);
            solve(&lib, &mut st, &a);
            continue;
        }
        if st.project.as_ref().is_some_and(|p| p.is_complete()) {
            break;
        }
        st.advance_week(&lib);
        if let Some(p) = &st.project {
            if phases.last() != Some(&p.phase()) {
                phases.push(p.phase());
            }
        }
    }
    assert!(saw_blocker, "a Small project has blocking challenges");
    assert_eq!(phases, Phase::ALL.to_vec(), "all four phases are visited in order");
    let p = st.project.as_ref().unwrap();
    assert!(p.is_complete());
    assert_eq!(p.open_blockers(), 0);
    assert!(p.challenge_quality > 0.0, "solving blockers adds quality");

    let money_before = st.studio.money;
    let idx = st.release_project(&lib).unwrap();
    assert!(st.project.is_none());
    let g = &st.games[idx];
    assert_eq!(g.reviews.len(), lib.outlets.len());
    assert!((0.0..=100.0).contains(&g.metascore));
    assert_eq!(st.studio.money, money_before, "release itself is free; money comes from sales");

    run_until(&lib, &mut st, 60, |_| false);
    let g = &st.games[idx];
    assert!(g.units_total > 0 && g.revenue_total > 0, "the game sells");
    assert!(!g.on_sale, "sales eventually run out");
    assert_eq!(g.sales.iter().map(|u| *u as u64).sum::<u64>(), g.units_total);
    assert!(st.studio.reputation > 0, "a released game earns fans");
    assert_eq!(st.progress.stat("games_released"), 1);
}

#[test]
fn cannot_release_unfinished_or_blocked_games() {
    let lib = content();
    let mut st = new_state(&lib);
    assert!(st.release_project(&lib).is_err());
    st.start_project(&lib, puzzle_config(&lib)).unwrap();
    assert!(st.release_project(&lib).is_err());
}

#[test]
fn time_stops_while_a_challenge_is_pending() {
    let lib = content();
    let mut st = new_state(&lib);
    st.start_project(&lib, puzzle_config(&lib)).unwrap();
    for _ in 0..200 {
        st.advance_week(&lib);
        if st.pending_attempt.is_some() {
            break;
        }
    }
    let attempt = st.pending_attempt.clone().expect("a blocker appears");
    let (week, money) = (st.date.week(), st.studio.money);
    st.advance_week(&lib);
    st.advance_week(&lib);
    assert_eq!((st.date.week(), st.studio.money), (week, money));
    solve(&lib, &mut st, &attempt);
    st.advance_week(&lib);
    assert_eq!(st.date.week(), week + 1);
}

#[test]
fn crunch_is_faster_but_costs_morale_and_adds_bugs() {
    let lib = content();
    let run = |crunch: bool| {
        let mut st = new_state(&lib);
        st.start_project(&lib, puzzle_config(&lib)).unwrap();
        st.set_crunch(crunch);
        for _ in 0..6 {
            if st.pending_attempt.is_some() {
                break;
            }
            st.advance_week(&lib);
        }
        st
    };
    let (calm, crunched) = (run(false), run(true));
    assert!(crunched.project.as_ref().unwrap().work_done > calm.project.as_ref().unwrap().work_done);
    assert!(crunched.staff[0].morale < calm.staff[0].morale);
    assert!(crunched.project.as_ref().unwrap().crunch_bugs > 0.0);
    assert_eq!(calm.project.as_ref().unwrap().crunch_bugs, 0.0);
}

#[test]
fn going_broke_triggers_warnings_then_game_over() {
    let lib = content();
    let mut st = new_state(&lib);
    st.studio.money = -10;
    let limit = lib.balance.bankruptcy_weeks;
    for _ in 0..limit - 1 {
        st.advance_week(&lib);
        assert!(st.game_over.is_none());
    }
    assert!(st.feed.iter().any(|n| n.text.contains("Bankruptcy warning")));
    st.advance_week(&lib);
    assert!(st.game_over.is_some());
    let week = st.date.week();
    st.advance_week(&lib);
    assert_eq!(st.date.week(), week, "the clock stops after game over");
}

#[test]
fn recovering_cash_resets_the_bankruptcy_clock() {
    let lib = content();
    let mut st = new_state(&lib);
    st.studio.money = -10;
    st.advance_week(&lib);
    st.advance_week(&lib);
    assert_eq!(st.debt_weeks, 2);
    st.studio.money = 5_000;
    st.advance_week(&lib);
    assert_eq!(st.debt_weeks, 0);
}

#[test]
fn full_game_state_round_trips_through_a_save_file() {
    let lib = content();
    let mut st = new_state(&lib);
    st.start_project(&lib, puzzle_config(&lib)).unwrap();
    run_until(&lib, &mut st, 100, |s| s.project.as_ref().is_some_and(|p| p.is_complete()));
    st.release_project(&lib).unwrap();
    run_until(&lib, &mut st, 10, |_| false);
    st.start_project(&lib, puzzle_config(&lib)).unwrap();
    for _ in 0..40 {
        st.advance_week(&lib);
        if st.pending_attempt.is_some() {
            break;
        }
    }
    let dir = std::env::temp_dir().join(format!("rst_sim_save_{}", std::process::id()));
    let paths = studio_core::paths::AppPaths::at(&dir);
    save::save_game(&paths, "slot1", &st).unwrap();
    let loaded = save::load_game(&paths, "slot1").unwrap().state;
    assert_eq!(loaded, st);
    // The loaded game continues identically.
    let mut a = st.clone();
    let mut b = loaded;
    if let Some(att) = a.pending_attempt.clone() {
        solve(&lib, &mut a, &att);
        solve(&lib, &mut b, &att);
    }
    for _ in 0..5 {
        a.advance_week(&lib);
        b.advance_week(&lib);
    }
    assert_eq!(a, b);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn better_teams_make_better_games() {
    let lib = content();
    let play = |skill: f32| {
        let mut st = new_state(&lib);
        for s in st.staff.iter_mut() {
            s.programming = skill;
            s.design = skill;
            s.art = skill;
            s.audio = skill;
        }
        st.start_project(&lib, puzzle_config(&lib)).unwrap();
        run_until(&lib, &mut st, 150, |s| s.project.as_ref().is_some_and(|p| p.is_complete()));
        let idx = st.release_project(&lib).unwrap();
        st.games[idx].metascore
    };
    let (weak, mid, strong) = (play(2.0), play(5.0), play(8.0));
    assert!(weak + 5.0 < mid && mid + 5.0 < strong, "{weak} < {mid} < {strong}");
}

#[test]
fn matching_the_genre_focus_beats_ignoring_it() {
    let lib = content();
    let play = |focus: Weights| {
        let mut st = new_state(&lib);
        let mut cfg = puzzle_config(&lib);
        cfg.focus = focus;
        st.start_project(&lib, cfg).unwrap();
        run_until(&lib, &mut st, 150, |s| s.project.as_ref().is_some_and(|p| p.is_complete()));
        let idx = st.release_project(&lib).unwrap();
        st.games[idx].quality
    };
    let ideal = lib.genres.iter().find(|g| g.id == "puzzle").unwrap().ideal;
    let wrong = Weights::new(5.0, 5.0, 85.0, 3.0, 2.0);
    assert!(play(ideal) > play(wrong) + 4.0);
}

#[test]
fn engine_modules_gate_genres_and_studio_tier_gates_sizes() {
    let lib = content();
    let mut st = new_state(&lib);
    st.studio.money = 10_000_000;
    let mut cfg = puzzle_config(&lib);
    cfg.genre = "platformer".into();
    cfg.focus = lib.genre("platformer").unwrap().ideal;
    let problem = st.project_start_problem(&lib, &cfg).expect("platformer needs the 2D renderer");
    assert!(problem.contains("renderer_2d"), "{problem}");
    st.engine.built.insert("asset_manager".into());
    st.engine.built.insert("renderer_2d".into());
    assert!(st.project_start_problem(&lib, &cfg).is_none());

    // Size is gated by studio tier AND engine features.
    cfg.size = ProjectSize::Medium;
    assert!(st.project_start_problem(&lib, &cfg).unwrap().contains("Garage"));
    st.studio.tier = 1;
    assert!(st.project_start_problem(&lib, &cfg).is_none());
    cfg.size = ProjectSize::AAA;
    st.studio.tier = 3;
    assert!(st.project_start_problem(&lib, &cfg).unwrap().contains("renderer_3d"));
}

#[test]
fn a_bigger_team_finishes_sooner_but_costs_more() {
    let lib = content();
    let play = |extra_staff: usize| {
        let mut st = new_state(&lib);
        st.studio.tier = 2;
        st.studio.money = 10_000_000;
        for i in 0..extra_staff {
            st.staff.push(studio_core::sim::Staff { id: 100 + i as u32, salary: 900, ..Default::default() });
        }
        st.start_project(&lib, puzzle_config(&lib)).unwrap();
        let start = st.date.week();
        run_until(&lib, &mut st, 200, |s| s.project.as_ref().is_some_and(|p| p.is_complete()));
        (st.date.week() - start, lib.balance.start_money.max(10_000_000) - st.studio.money)
    };
    let (solo_weeks, solo_cost) = play(0);
    let (team_weeks, team_cost) = play(3);
    assert!(team_weeks * 2 < solo_weeks + 3, "{team_weeks} vs {solo_weeks}");
    assert!(team_cost > 0 && solo_cost > 0);
}

#[test]
fn morale_effects_on_output_are_visible_in_the_team_snapshot() {
    let lib = content();
    let mut st = new_state(&lib);
    let happy = st.team_week(&lib, false).output;
    st.staff[0].morale = 5.0;
    let sad = st.team_week(&lib, false).output;
    assert!(sad < happy * 0.8, "{sad} vs {happy}");
}

#[test]
fn every_engine_module_is_reachable_from_the_start() {
    // Following dependencies from the free starting module, every module can eventually be built.
    let lib = content();
    let mut built: std::collections::HashSet<String> =
        lib.engine_modules.iter().filter(|m| m.starts_built).map(|m| m.id.clone()).collect();
    loop {
        let next: Vec<String> = lib
            .engine_modules
            .iter()
            .filter(|m| !built.contains(&m.id) && m.requires_modules.iter().all(|r| built.contains(r)))
            .map(|m| m.id.clone())
            .collect();
        if next.is_empty() {
            break;
        }
        built.extend(next);
    }
    for m in &lib.engine_modules {
        assert!(built.contains(&m.id), "module {} is unreachable", m.id);
    }
}

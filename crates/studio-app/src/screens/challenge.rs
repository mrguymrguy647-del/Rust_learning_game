//! The challenge screen: story/task/hints on the left, code editor + results on the right.

use std::time::Instant;

use eframe::egui::{self, RichText};
use egui_extras::syntax_highlighting::{code_view_ui, CodeTheme};
use studio_core::challenges::testparse::TestStatus;
use studio_core::challenges::{
    grade_quiz, spawn_run, Level, Phase, RunEvent, RunHandle, RunReport, Verdict, INSTALL_INSTRUCTIONS,
};
use studio_core::data::Challenge;
use studio_core::sim::{Attempt, ChallengeContext, HintPayment, RewardSummary};

use crate::app::{App, ToastKind, ToolchainStatus};
use crate::editor::{CodeEditor, Marker};
use crate::theme::Palette;
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultTab {
    Summary,
    Compiler,
    Tests,
    Output,
}

/// Everything about the challenge currently on screen.
pub struct ChallengeView {
    pub challenge: Challenge,
    pub attempt: Attempt,
    run: Option<RunHandle>,
    phase: Phase,
    run_started: Option<Instant>,
    report: Option<RunReport>,
    tab: ResultTab,
    quiz_choice: Option<usize>,
    quiz_message: Option<String>,
    hint_texts: Vec<String>,
    confirm_contractor: bool,
    /// Present once the challenge is finished (solved, or solved by a contractor).
    solved: Option<RewardSummary>,
    /// True when the player asked for the solution in practice mode.
    show_solution: bool,
    /// Where to return when the challenge is closed.
    pub return_to: crate::app::Nav,
    /// The jam timer ran out: the attempt is over.
    expired: bool,
}

impl ChallengeView {
    pub fn new(challenge: Challenge, attempt: Attempt, return_to: crate::app::Nav) -> ChallengeView {
        let hint_texts = challenge.hints.iter().take(attempt.hints_revealed).cloned().collect();
        ChallengeView {
            challenge,
            attempt,
            run: None,
            phase: Phase::Preparing,
            run_started: None,
            report: None,
            tab: ResultTab::Summary,
            quiz_choice: None,
            quiz_message: None,
            hint_texts,
            confirm_contractor: false,
            solved: None,
            show_solution: false,
            return_to,
            expired: false,
        }
    }

    fn running(&self) -> bool {
        self.run.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Close,
    Submit,
    Cancel,
    Reset,
    Hint(HintPayment),
    Contractor,
    SubmitQuiz,
    ShowSolution,
    /// The game-jam clock ran out.
    JamExpired,
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(mut view) = app.challenge.take() else {
        return;
    };
    poll_run(app, &mut view);

    // Game-jam countdown (real time).
    let mut action: Option<Action> = None;
    if view.solved.is_none() && !view.expired {
        if let Some(limit) = view.attempt.time_limit_secs {
            view.attempt.elapsed_secs += ui.input(|i| i.stable_dt).min(0.25);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            if view.attempt.elapsed_secs >= limit as f32 {
                action = Some(Action::JamExpired);
            }
        }
    }

    let pal = Palette::of(ui);

    egui::Panel::top("challenge_header").show(ui, |ui| {
        ui.add_space(4.0);
        action = action.or(header(app, &mut view, ui));
        ui.add_space(4.0);
    });

    egui::Panel::left("challenge_left")
        .resizable(true)
        .default_size(430.0)
        .size_range(320.0..=700.0)
        .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::same(12)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                action = action.or(left_pane(app, &mut view, ui));
            });
        });

    egui::CentralPanel::default_margins().show(ui, |ui| {
        if let Some(rewards) = view.solved.clone() {
            action = action.or(solved_pane(app, &view, &rewards, ui));
        } else if view.challenge.is_quiz() {
            action = action.or(quiz_pane(&mut view, ui));
        } else {
            action = action.or(code_pane(app, &mut view, ui));
        }
    });

    if view.running() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    apply(app, &mut view, action);
    if let Some(v) = app.challenge.take() {
        // `apply` may have replaced it (it never does today) — keep the newest.
        app.challenge = Some(v);
    } else if !matches!(action, Some(Action::Close) | Some(Action::JamExpired)) {
        app.challenge = Some(view);
    } else {
        // Closing: unsolved blocking attempts stay in the save so they can be resumed.
        let blocking = matches!(
            view.attempt.context,
            ChallengeContext::Project | ChallengeContext::Hotfix { .. } | ChallengeContext::Jam { .. }
        );
        if let Some(game) = app.game.as_mut() {
            if blocking && view.solved.is_none() && !view.expired {
                game.pending_attempt = Some(view.attempt.clone());
            } else {
                game.pending_attempt = None;
            }
        }
        app.stash_draft(&view.challenge.id, &view.attempt.code, view.solved.is_some());
        app.nav = view.return_to;
    }
}

fn poll_run(app: &mut App, view: &mut ChallengeView) {
    let Some(handle) = view.run.as_mut() else {
        return;
    };
    let mut finished: Option<RunReport> = None;
    for ev in handle.poll() {
        match ev {
            RunEvent::Phase(p) => view.phase = p,
            RunEvent::Done(report) => finished = Some(*report),
        }
    }
    if let Some(report) = finished {
        view.run = None;
        view.run_started = None;
        view.tab = match report.verdict {
            Verdict::CompileError => ResultTab::Compiler,
            Verdict::TestsFailed | Verdict::CheckFailed => ResultTab::Tests,
            _ => ResultTab::Summary,
        };
        match &report.verdict {
            Verdict::Passed => {
                finish(app, view, false);
            }
            Verdict::Cancelled | Verdict::Unavailable(_) => {}
            _ => view.attempt.failed_submissions += 1,
        }
        view.report = Some(report);
    }
}

/// Mark the challenge done and pay out.
fn finish(app: &mut App, view: &mut ChallengeView, contractor: bool) {
    let content = app.content.clone();
    let Some(game) = app.game.as_mut() else {
        // No game running (should not happen): still show success.
        view.solved = Some(RewardSummary { first_try: view.attempt.failures() == 0, ..Default::default() });
        return;
    };
    view.attempt.contractor = view.attempt.contractor || contractor;
    let rewards = game.complete_challenge(&content, &view.challenge, &view.attempt);
    game.pending_attempt = None;
    for name in &rewards.new_achievements {
        app.toasts.push(crate::app::Toast {
            text: format!("Achievement unlocked: {name}"),
            kind: ToastKind::Good,
            age: 0.0,
        });
    }
    if let Some(level) = rewards.level_up {
        app.toasts.push(crate::app::Toast {
            text: format!("Level up! You are now level {level}"),
            kind: ToastKind::Good,
            age: 0.0,
        });
    }
    view.solved = Some(rewards);
}

fn apply(app: &mut App, view: &mut ChallengeView, action: Option<Action>) {
    let Some(action) = action else {
        return;
    };
    match action {
        Action::Close => {}
        Action::JamExpired => {
            view.expired = true;
            let content = app.content.clone();
            if let (Some(game), ChallengeContext::Jam { event_id }) =
                (app.game.as_mut(), view.attempt.context.clone())
            {
                game.fail_jam(&content, &event_id);
            }
            app.toast(ToastKind::Warn, "Time is up! The jam is over.");
        }
        Action::Cancel => {
            if let Some(run) = &view.run {
                run.cancel();
            }
        }
        Action::Reset => {
            view.attempt.code = view.challenge.starter_code.clone();
            view.report = None;
        }
        Action::Submit => submit(app, view),
        Action::Hint(payment) => {
            let multiplier = app.settings.hint_cost_multiplier;
            let challenge = view.challenge.clone();
            let content = app.content.clone();
            let text = match app.game.as_mut() {
                Some(game) => game.buy_hint(&content, &challenge, &mut view.attempt, payment, multiplier),
                None => None,
            };
            match text {
                Some(t) => view.hint_texts.push(t),
                None => app.toast(ToastKind::Warn, "You cannot afford that hint right now."),
            }
        }
        Action::Contractor => {
            let challenge = view.challenge.clone();
            let content = app.content.clone();
            if let Some(game) = app.game.as_mut() {
                let cost = game.hire_contractor(&content, &challenge, &mut view.attempt);
                if cost > 0 {
                    app.toast(
                        ToastKind::Info,
                        format!("Contractor hired for {}", studio_core::fmt::money(cost)),
                    );
                }
            }
            view.confirm_contractor = false;
            finish(app, view, true);
        }
        Action::ShowSolution => {
            view.show_solution = true;
        }
        Action::SubmitQuiz => {
            let Some(choice) = view.quiz_choice else {
                return;
            };
            if grade_quiz(&view.challenge, choice) {
                finish(app, view, false);
            } else {
                view.attempt.quiz_wrong_guesses += 1;
                view.quiz_message = Some("Not quite. Think it through again — or buy a hint.".to_string());
            }
        }
    }
    // Keep a blocking attempt in the save so that quitting mid-challenge does not lose it.
    if let Some(game) = app.game.as_mut() {
        if view.solved.is_none()
            && matches!(
                view.attempt.context,
                ChallengeContext::Project | ChallengeContext::Hotfix { .. } | ChallengeContext::Jam { .. }
            )
        {
            game.pending_attempt = Some(view.attempt.clone());
        }
    }
}

fn submit(app: &mut App, view: &mut ChallengeView) {
    if view.running() {
        return;
    }
    let ToolchainStatus::Ready(runner) = &app.toolchain else {
        app.toast(ToastKind::Warn, "No Rust toolchain detected — see the notice above the editor.");
        return;
    };
    runner.set_config(studio_core::challenges::RunnerConfig::from_settings(&app.settings));
    view.attempt.submissions += 1;
    view.report = None;
    view.phase = Phase::Preparing;
    view.run_started = Some(Instant::now());
    let ctx = app.egui_ctx.clone();
    view.run =
        Some(spawn_run(runner.clone(), view.challenge.clone(), view.attempt.code.clone(), move || {
            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
        }));
}

// ---------------------------------------------------------------------------------------------

fn stars(difficulty: u8) -> String {
    let d = difficulty.clamp(1, 5) as usize;
    format!("{}{}", "★".repeat(d), "☆".repeat(5 - d))
}

fn header(app: &mut App, view: &mut ChallengeView, ui: &mut egui::Ui) -> Option<Action> {
    let pal = Palette::of(ui);
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui.button("Back").clicked() {
            action = Some(Action::Close);
        }
        ui.label(RichText::new(&view.challenge.title).strong().size(19.0));
        ui.label(RichText::new(view.challenge.kind.label()).color(pal.accent));
        ui.label(RichText::new(stars(view.challenge.difficulty)).color(pal.warn));
        widgets::chip(ui, "Mode", view.attempt.context.label(), pal.info);
        if let Some(limit) = view.attempt.time_limit_secs {
            let left = (limit as f32 - view.attempt.elapsed_secs).max(0.0) as u32;
            let color = if left < 60 { pal.bad } else { pal.warn };
            widgets::chip(ui, "Time left", &format!("{}:{:02}", left / 60, left % 60), color);
        }
        if let Some(game) = &app.game {
            if !view.attempt.context.is_practice() {
                let color = if game.studio.money < 0 { pal.bad } else { pal.good };
                widgets::chip(ui, "Cash", &studio_core::fmt::money(game.studio.money), color);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if view.solved.is_none() && !view.challenge.is_quiz() {
                if view.running() {
                    if ui.button("Cancel").clicked() {
                        action = Some(Action::Cancel);
                    }
                    ui.spinner();
                    let secs = view.run_started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                    ui.label(format!("{} {secs}s", view.phase.label()));
                } else {
                    let ready = matches!(app.toolchain, ToolchainStatus::Ready(_));
                    let button =
                        egui::Button::new(RichText::new("Compile & Test").strong().color(pal.accent_text))
                            .fill(if ready { pal.accent } else { pal.dim })
                            .min_size(egui::vec2(0.0, 30.0));
                    if ui.add_enabled(ready, button).clicked() {
                        action = Some(Action::Submit);
                    }
                    if ui.button("Reset code").clicked() {
                        action = Some(Action::Reset);
                    }
                }
            }
            ui.label(
                RichText::new(format!(
                    "Attempts: {}",
                    view.attempt.submissions + view.attempt.quiz_wrong_guesses
                ))
                .color(pal.dim),
            );
        });
    });
    action
}

fn left_pane(app: &mut App, view: &mut ChallengeView, ui: &mut egui::Ui) -> Option<Action> {
    let pal = Palette::of(ui);
    let mut action = None;

    ui.label(RichText::new("THE SITUATION").small().color(pal.dim));
    ui.label(RichText::new(&view.challenge.story).italics());
    ui.add_space(10.0);
    ui.label(RichText::new("YOUR TASK").small().color(pal.dim));
    ui.label(&view.challenge.task);

    if !view.challenge.signatures.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("REQUIRED SIGNATURES").small().color(pal.dim));
        widgets::card(ui, |ui| {
            for sig in &view.challenge.signatures {
                ui.label(RichText::new(sig).monospace().color(pal.accent));
            }
        });
    }
    if !view.challenge.checks.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("STYLE REQUIREMENTS").small().color(pal.dim));
        for c in &view.challenge.checks {
            ui.label(format!("• {}", c.message()));
        }
    }

    ui.add_space(14.0);
    widgets::section(ui, "Hints");
    let total_hints = view.challenge.hints.len();
    for (i, text) in view.hint_texts.iter().enumerate() {
        widgets::card(ui, |ui| {
            ui.label(RichText::new(format!("Hint {}", i + 1)).strong().color(pal.accent));
            if text.contains('\n') || text.contains("fn ") || text.contains(';') && text.contains("let ") {
                ui.label(RichText::new(text).monospace());
            } else {
                ui.label(text);
            }
        });
    }
    if view.attempt.hints_revealed < total_hints && view.solved.is_none() {
        let tier = view.attempt.hints_revealed;
        let multiplier = app.settings.hint_cost_multiplier;
        let content = app.content.clone();
        let senior_free = app.game.as_ref().map(|g| g.free_hints(&content)).unwrap_or(0);
        let (cost, money) = match &app.game {
            Some(g) => {
                (g.hint_cost(&content, &view.challenge, tier, multiplier, &view.attempt), g.studio.money)
            }
            None => (0, 0),
        };
        let label =
            ["concept nudge", "specific direction", "partial code"].get(tier).copied().unwrap_or("hint");
        ui.horizontal_wrapped(|ui| {
            if cost == 0 {
                let why = if tier < senior_free { " (covered by your senior developers)" } else { "" };
                if ui.button(format!("Reveal hint {} ({label}) — free{why}", tier + 1)).clicked() {
                    action = Some(Action::Hint(HintPayment::Money));
                }
            } else {
                let afford = money >= cost;
                if ui
                    .add_enabled(
                        afford,
                        egui::Button::new(format!(
                            "Hint {} ({label}) — {}",
                            tier + 1,
                            studio_core::fmt::money(cost)
                        )),
                    )
                    .clicked()
                {
                    action = Some(Action::Hint(HintPayment::Money));
                }
                if ui.button("…or spend a week researching").clicked() {
                    action = Some(Action::Hint(HintPayment::Time));
                }
            }
        });
    } else if view.hint_texts.is_empty() && view.solved.is_none() {
        widgets::dim(ui, "No hints for this challenge.");
    }

    if view.solved.is_none() {
        ui.add_space(14.0);
        widgets::section(ui, "Stuck?");
        let threshold = app.settings.contractor_after_failures;
        if view.attempt.context.is_practice() {
            if view.show_solution {
                widgets::dim(ui, "The reference solution is shown below the editor.");
            } else if ui.button("Reveal the reference solution").clicked() {
                action = Some(Action::ShowSolution);
            }
        } else if view.attempt.contractor_available(threshold) {
            let cost =
                app.game.as_ref().map(|g| g.contractor_price(&app.content, &view.challenge)).unwrap_or(0);
            widgets::dim(
                ui,
                "A contractor will solve this for you. It is expensive and counts only a little towards mastery — but you will still see the solution and its explanation.",
            );
            if view.confirm_contractor {
                ui.horizontal(|ui| {
                    if ui.button(format!("Yes, pay {}", studio_core::fmt::money(cost))).clicked() {
                        action = Some(Action::Contractor);
                    }
                    if ui.button("No, keep trying").clicked() {
                        view.confirm_contractor = false;
                    }
                });
            } else if ui.button(format!("Hire a contractor ({})", studio_core::fmt::money(cost))).clicked() {
                view.confirm_contractor = true;
            }
        } else {
            widgets::dim(
                ui,
                format!(
                    "A contractor can be hired after {} failed attempts ({} so far). You can retry as often as you like.",
                    threshold,
                    view.attempt.failures()
                ),
            );
        }
    }
    action
}

fn code_pane(app: &mut App, view: &mut ChallengeView, ui: &mut egui::Ui) -> Option<Action> {
    let pal = Palette::of(ui);
    let mut action = None;

    // Toolchain missing: explain, never crash.
    match &app.toolchain {
        ToolchainStatus::Detecting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Detecting your Rust toolchain…");
            });
        }
        ToolchainStatus::Missing(err) => {
            let err = err.to_string();
            egui::Frame::new()
                .fill(pal.warn.gamma_multiply(0.15))
                .stroke(egui::Stroke::new(1.0, pal.warn))
                .corner_radius(8)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Rust toolchain not available: {err}"))
                            .strong()
                            .color(pal.warn),
                    );
                    ui.label(INSTALL_INSTRUCTIONS);
                    if ui.button("Detect again").clicked() {
                        app.redetect_toolchain();
                    }
                });
            ui.add_space(6.0);
        }
        ToolchainStatus::Ready(_) => {}
    }

    // Markers from the last compile.
    let mut markers: Vec<(u32, Marker)> = Vec::new();
    if let Some(report) = &view.report {
        for d in &report.diagnostics {
            if let Some(line) = d.line {
                let marker = if d.level == Level::Error { Marker::Error } else { Marker::Warning };
                markers.push((line, marker));
            }
        }
    }

    let total_height = ui.available_height();
    let editor_height = (total_height * 0.58).max(220.0);
    let id = egui::Id::new(("code_editor", view.challenge.id.clone()));
    ui.allocate_ui(egui::vec2(ui.available_width(), editor_height), |ui| {
        let enabled = !view.running();
        ui.add_enabled_ui(enabled, |ui| {
            CodeEditor {
                code: &mut view.attempt.code,
                id,
                markers: &markers,
                min_height: editor_height - 20.0,
            }
            .show(ui);
        });
    });

    ui.add_space(6.0);
    results_pane(app, view, ui);

    if view.show_solution {
        ui.add_space(8.0);
        widgets::section(ui, "Reference solution");
        let theme = CodeTheme::from_style(ui.style());
        code_view_ui(ui, &theme, &view.challenge.solution, "rs");
    }
    if matches!(view.report.as_ref().map(|r| &r.verdict), Some(Verdict::Passed)) {
        action = None;
    }
    action
}

fn results_pane(app: &mut App, view: &mut ChallengeView, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut view.tab, ResultTab::Summary, "Result");
        let n_err = view.report.as_ref().map(|r| r.errors().count()).unwrap_or(0);
        let n_warn = view.report.as_ref().map(|r| r.warnings().count()).unwrap_or(0);
        ui.selectable_value(
            &mut view.tab,
            ResultTab::Compiler,
            format!("Compiler ({n_err} errors, {n_warn} warnings)"),
        );
        let tests = view.report.as_ref().and_then(|r| r.tests.as_ref());
        let label = match tests {
            Some(t) => format!("Tests ({}/{})", t.passed(), t.cases.len()),
            None => "Tests".to_string(),
        };
        ui.selectable_value(&mut view.tab, ResultTab::Tests, label);
        ui.selectable_value(&mut view.tab, ResultTab::Output, "Raw output");
    });
    ui.separator();

    egui::ScrollArea::vertical().id_salt("results_scroll").auto_shrink([false, false]).show(ui, |ui| {
        if view.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(view.phase.label());
            });
            widgets::dim(ui, "The first build in a fresh sandbox can take a while; later checks are fast.");
            return;
        }
        let Some(report) = view.report.clone() else {
            widgets::dim(ui, "Press “Compile & Test” to check your code with the real Rust compiler.");
            return;
        };
        match view.tab {
            ResultTab::Summary => summary_tab(app, &report, ui),
            ResultTab::Compiler => compiler_tab(app, &report, ui),
            ResultTab::Tests => tests_tab(&report, ui),
            ResultTab::Output => {
                if !report.build_log.is_empty() {
                    ui.label(RichText::new("cargo:").strong());
                    ui.label(RichText::new(&report.build_log).monospace());
                }
                if report.test_output.is_empty() && report.build_log.is_empty() {
                    widgets::dim(ui, "No program output.");
                } else {
                    ui.label(RichText::new(&report.test_output).monospace().color(pal.dim));
                }
                if report.output_truncated {
                    ui.label(RichText::new("(output was cut off at the sandbox limit)").color(pal.warn));
                }
            }
        }
    });
}

fn verdict_banner(ui: &mut egui::Ui, report: &RunReport) {
    let pal = Palette::of(ui);
    let (color, title) = match &report.verdict {
        Verdict::Passed => (pal.good, "All tests passed!"),
        Verdict::CompileError => (pal.bad, "Compile error"),
        Verdict::TestsFailed => (pal.bad, "Some tests failed"),
        Verdict::Timeout => (pal.warn, "Timed out"),
        Verdict::Crashed => (pal.bad, "The program crashed"),
        Verdict::CheckFailed => (pal.warn, "Tests pass — but the code style requirements are not met"),
        Verdict::Cancelled => (pal.dim, "Cancelled"),
        Verdict::Unavailable(_) => (pal.warn, "Could not run"),
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong().size(16.0).color(color));
                ui.label(
                    RichText::new(format!("{} ms compile · {} ms tests", report.compile_ms, report.test_ms))
                        .small()
                        .color(pal.dim),
                );
            });
        });
}

fn summary_tab(app: &App, report: &RunReport, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    verdict_banner(ui, report);
    ui.add_space(4.0);
    match &report.verdict {
        Verdict::CompileError => {
            ui.label("Your code did not compile. Open the “Compiler” tab for the details; the offending lines are marked in the editor.");
            if let Some(first) = report.errors().next() {
                diagnostic_card(app, first, ui);
            }
        }
        Verdict::TestsFailed | Verdict::CheckFailed => tests_tab(report, ui),
        Verdict::Timeout | Verdict::Crashed => {
            if let Some(note) = &report.note {
                ui.label(RichText::new(note).color(pal.warn));
            }
            tests_tab(report, ui);
        }
        Verdict::Unavailable(why) => {
            ui.label(RichText::new(why).color(pal.warn));
            if !report.build_log.is_empty() {
                ui.label(RichText::new(&report.build_log).monospace());
            }
        }
        Verdict::Passed | Verdict::Cancelled => {}
    }
}

fn diagnostic_card(app: &App, d: &studio_core::challenges::Diagnostic, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    let is_error = d.level == Level::Error;
    let color = if is_error { pal.bad } else { pal.warn };
    widgets::card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if let Some(code) = &d.code {
                ui.label(RichText::new(code).strong().monospace().color(color));
            } else {
                ui.label(RichText::new(if is_error { "error" } else { "warning" }).strong().color(color));
            }
            match (d.line, d.column) {
                (Some(l), Some(c)) => {
                    ui.label(RichText::new(format!("line {l}, col {c}")).color(pal.dim));
                }
                _ if d.in_hidden_tests => {
                    ui.label(RichText::new("in the hidden tests").color(pal.dim));
                }
                _ => {}
            }
        });
        ui.label(RichText::new(&d.message).strong());
        if let Some(label) = &d.label {
            ui.label(RichText::new(format!("» {label}")).color(pal.dim));
        }
        if d.in_hidden_tests {
            ui.label(RichText::new("The hidden tests could not compile against your code. This almost always means a function name, parameter or return type does not match the required signature.").color(pal.warn));
        }
        if let Some(code) = &d.code {
            if let Some(ex) = app.content.explainer(code) {
                egui::Frame::new()
                    .fill(pal.info.gamma_multiply(0.12))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("In plain English — {}", ex.title))
                                .strong()
                                .color(pal.info),
                        );
                        ui.label(&ex.explanation);
                        ui.label(RichText::new(format!("How to fix it: {}", ex.how_to_fix)).italics());
                        if !ex.book_url.is_empty() {
                            ui.hyperlink_to("Read more in the Rust Book", &ex.book_url);
                        }
                    });
            }
        }
        for n in &d.notes {
            ui.label(RichText::new(n).small().color(pal.dim));
        }
        egui::CollapsingHeader::new("Full compiler message").id_salt((d.line, &d.message)).show(ui, |ui| {
            ui.label(RichText::new(d.rendered.trim()).monospace().small());
        });
    });
}

fn compiler_tab(app: &App, report: &RunReport, ui: &mut egui::Ui) {
    if report.diagnostics.is_empty() {
        let pal = Palette::of(ui);
        ui.label(RichText::new("No compiler messages. Clean build!").color(pal.good));
        return;
    }
    for d in report.errors() {
        diagnostic_card(app, d, ui);
    }
    for d in report.warnings() {
        diagnostic_card(app, d, ui);
    }
}

fn tests_tab(report: &RunReport, ui: &mut egui::Ui) {
    let pal = Palette::of(ui);
    if let Some(t) = &report.tests {
        for case in &t.cases {
            let (icon, color) = match case.status {
                TestStatus::Passed => ("✔", pal.good),
                TestStatus::Failed => ("✖", pal.bad),
                TestStatus::Ignored => ("–", pal.dim),
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).strong().color(color));
                ui.label(RichText::new(&case.name).monospace());
            });
            if let Some(msg) = &case.message {
                egui::Frame::new()
                    .fill(pal.bad.gamma_multiply(0.1))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).monospace().small());
                        if let Some(loc) = &case.location {
                            ui.label(RichText::new(format!("at {loc}")).small().color(pal.dim));
                        }
                    });
            }
        }
        if let Some(name) = &t.unfinished {
            ui.label(
                RichText::new(format!("The test `{name}` was still running when the program was stopped."))
                    .color(pal.warn),
            );
        }
    }
    for msg in &report.check_failures {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("✖").strong().color(pal.warn));
            ui.label(msg);
        });
    }
    if report.tests.is_none() && report.check_failures.is_empty() {
        widgets::dim(ui, "The tests did not run.");
    }
}

fn quiz_pane(view: &mut ChallengeView, ui: &mut egui::Ui) -> Option<Action> {
    let pal = Palette::of(ui);
    let mut action = None;
    let Some(quiz) = view.challenge.quiz.clone() else {
        ui.label("This quiz has no content.");
        return None;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.label(RichText::new(&quiz.question).strong().size(18.0));
        if !quiz.code.is_empty() {
            ui.add_space(6.0);
            let theme = CodeTheme::from_style(ui.style());
            widgets::card(ui, |ui| {
                code_view_ui(ui, &theme, &quiz.code, "rs");
            });
        }
        ui.add_space(10.0);
        for (i, option) in quiz.options.iter().enumerate() {
            let selected = view.quiz_choice == Some(i);
            let letter = (b'A' + i as u8) as char;
            let frame = egui::Frame::new()
                .fill(if selected { pal.accent.gamma_multiply(0.18) } else { pal.card })
                .stroke(egui::Stroke::new(
                    if selected { 1.5 } else { 1.0 },
                    if selected { pal.accent } else { pal.border },
                ))
                .corner_radius(8)
                .inner_margin(egui::Margin::same(10));
            let response = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(format!("{letter}.")).strong().color(pal.accent));
                        if quiz.options_are_code {
                            let theme = CodeTheme::from_style(ui.style());
                            code_view_ui(ui, &theme, option, "rs");
                        } else if option.contains('\n') {
                            ui.label(RichText::new(option).monospace());
                        } else {
                            ui.label(option);
                        }
                    });
                })
                .response
                .interact(egui::Sense::click());
            if response.clicked() {
                view.quiz_choice = Some(i);
                view.quiz_message = None;
            }
            ui.add_space(4.0);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ready = view.quiz_choice.is_some();
            let button = egui::Button::new(RichText::new("Submit answer").strong().color(pal.accent_text))
                .fill(pal.accent);
            if ui.add_enabled(ready, button).clicked() {
                action = Some(Action::SubmitQuiz);
            }
            if let Some(msg) = &view.quiz_message {
                ui.label(RichText::new(msg).color(pal.warn));
            }
        });
    });
    action
}

fn solved_pane(app: &App, view: &ChallengeView, r: &RewardSummary, ui: &mut egui::Ui) -> Option<Action> {
    let pal = Palette::of(ui);
    let mut action = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let headline = if r.contractor {
            "Solved by a contractor"
        } else if r.first_try {
            "Solved on the first try!"
        } else {
            "Challenge solved!"
        };
        ui.label(RichText::new(headline).size(26.0).strong().color(if r.contractor {
            pal.warn
        } else {
            pal.good
        }));
        if view.attempt.context.is_practice() {
            widgets::dim(ui, "Practice mode: no rewards, nothing changed in your studio.");
        } else {
            ui.horizontal_wrapped(|ui| {
                if r.xp > 0 {
                    widgets::chip(ui, "XP", &format!("+{}", r.xp), pal.info);
                }
                if r.research > 0 {
                    widgets::chip(ui, "Research", &format!("+{}", r.research), pal.accent);
                }
                if r.dev_points > 0 {
                    widgets::chip(ui, "Dev progress", &format!("+{}", r.dev_points), pal.good);
                }
                if r.quality > 0.0 {
                    widgets::chip(ui, "Quality", &format!("+{:.1}", r.quality), pal.good);
                }
                if let Some(level) = r.level_up {
                    widgets::chip(ui, "Level up", &format!("Level {level}"), pal.warn);
                }
            });
            if !r.first_time {
                widgets::dim(ui, "You had already solved this one, so the reward is only a token amount.");
            }
            for a in &r.new_achievements {
                ui.label(RichText::new(format!("Achievement unlocked: {a}")).color(pal.warn));
            }
            for id in &r.new_codex {
                if let Some(e) = app.content.codex.iter().find(|e| &e.id == id) {
                    ui.label(RichText::new(format!("New Codex entry: {}", e.title)).color(pal.info));
                }
            }
        }
        ui.add_space(10.0);
        widgets::section(ui, "What you learned");
        ui.label(&view.challenge.explanation);
        if !view.challenge.solution.is_empty() && !view.challenge.is_quiz() {
            ui.add_space(8.0);
            widgets::section(
                ui,
                if r.contractor { "The contractor's solution" } else { "The idiomatic solution" },
            );
            let theme = CodeTheme::from_style(ui.style());
            widgets::card(ui, |ui| {
                code_view_ui(ui, &theme, &view.challenge.solution, "rs");
            });
        } else if view.challenge.is_quiz() && !view.challenge.solution.is_empty() {
            ui.add_space(8.0);
            widgets::section(ui, "The answer");
            ui.label(RichText::new(&view.challenge.solution).monospace());
        }
        if !view.challenge.book_links.is_empty() {
            ui.add_space(8.0);
            widgets::section(ui, "Keep reading");
            for link in &view.challenge.book_links {
                ui.hyperlink_to(&link.title, &link.url);
            }
        }
        ui.add_space(14.0);
        if widgets::primary_button(ui, "Continue").clicked() {
            action = Some(Action::Close);
        }
    });
    action
}

/// Developer aid: put the starter or reference solution in the editor and submit it.
pub fn dev_submit(app: &mut App, use_solution: bool) {
    let Some(mut view) = app.challenge.take() else {
        return;
    };
    if !view.challenge.is_quiz() {
        view.attempt.code =
            if use_solution { view.challenge.solution.clone() } else { view.challenge.starter_code.clone() };
        submit(app, &mut view);
    }
    app.challenge = Some(view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;

    fn game_app(name: &str) -> App {
        let mut app = test_app(name);
        app.new_game.studio = "Test".into();
        app.start_new_game();
        app
    }

    fn view_mut(app: &mut App) -> &mut ChallengeView {
        app.challenge.as_mut().expect("a challenge is open")
    }

    #[test]
    fn wrong_quiz_answer_counts_as_a_failure_and_right_answer_pays() {
        let mut app = game_app("quiz");
        app.open_challenge("basics_03_predict_shadowing", ChallengeContext::Study);
        let mut v = app.challenge.take().unwrap();
        v.quiz_choice = Some(2);
        apply(&mut app, &mut v, Some(Action::SubmitQuiz));
        assert_eq!(v.attempt.quiz_wrong_guesses, 1);
        assert!(v.solved.is_none());
        assert!(v.quiz_message.is_some());
        v.quiz_choice = Some(0);
        apply(&mut app, &mut v, Some(Action::SubmitQuiz));
        let rewards = v.solved.clone().expect("solved");
        assert!(!rewards.first_try, "a wrong guess forfeits the first-try bonus");
        assert!(rewards.xp > 0);
        let game = app.game.as_ref().unwrap();
        assert!(game.progress.is_solved("basics_03_predict_shadowing"));
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn buying_a_hint_costs_money_and_shows_the_text() {
        let mut app = game_app("hint");
        app.open_challenge("basics_01_score_counter", ChallengeContext::Study);
        let before = app.game.as_ref().unwrap().studio.money;
        let mut v = app.challenge.take().unwrap();
        apply(&mut app, &mut v, Some(Action::Hint(HintPayment::Money)));
        assert_eq!(v.hint_texts.len(), 1);
        assert_eq!(v.hint_texts[0], v.challenge.hints[0]);
        let after = app.game.as_ref().unwrap().studio.money;
        assert_eq!(before - after, v.challenge.hint_cost(0, 1.0));
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn contractor_solves_it_but_is_expensive_and_marked() {
        let mut app = game_app("contractor");
        app.open_challenge("basics_01_score_counter", ChallengeContext::Study);
        let mut v = app.challenge.take().unwrap();
        v.attempt.failed_submissions = 3;
        let before = app.game.as_ref().unwrap().studio.money;
        apply(&mut app, &mut v, Some(Action::Contractor));
        let rewards = v.solved.clone().expect("contractor finishes the challenge");
        assert!(rewards.contractor && !rewards.first_try);
        let game = app.game.as_ref().unwrap();
        assert_eq!(before - game.studio.money, game.contractor_price(&app.content, &v.challenge));
        assert!(game.progress.solved["basics_01_score_counter"].contractor);
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn practice_never_changes_the_economy() {
        let mut app = game_app("practice");
        app.open_challenge("basics_01_score_counter", ChallengeContext::Practice);
        let snapshot = app.game.clone().unwrap();
        let mut v = app.challenge.take().unwrap();
        apply(&mut app, &mut v, Some(Action::Hint(HintPayment::Money)));
        v.attempt.failed_submissions = 3;
        apply(&mut app, &mut v, Some(Action::Contractor));
        assert!(v.solved.is_some());
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.studio.money, snapshot.studio.money);
        assert_eq!(game.progress, snapshot.progress);
        let _ = std::fs::remove_dir_all(app.paths.root());
    }

    #[test]
    fn blocking_challenges_survive_save_and_load() {
        let mut app = game_app("resume");
        app.open_challenge("basics_01_score_counter", ChallengeContext::Project);
        view_mut(&mut app).attempt.code = "// my work in progress".into();
        let mut v = app.challenge.take().unwrap();
        apply(&mut app, &mut v, Some(Action::Hint(HintPayment::Money)));
        app.challenge = Some(v);
        assert!(app.game.as_ref().unwrap().pending_attempt.is_some());
        app.save_to_slot("slot1");
        app.exit_to_title();
        app.load_slot("slot1");
        let v = app.challenge.as_ref().expect("the pending challenge reopens after loading");
        assert_eq!(v.attempt.code, "// my work in progress");
        assert_eq!(v.attempt.hints_revealed, 1);
        assert_eq!(v.hint_texts.len(), 1);
        let _ = std::fs::remove_dir_all(app.paths.root());
    }
}

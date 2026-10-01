//! Content validation: *prove* every code challenge is solvable and not already solved.
//!
//! For each code challenge:
//! * the reference `solution` must be a full pass (compiles, tests pass, style checks pass);
//! * the `starter_code` must NOT be a full pass — and for some kinds, must fail in a specific way
//!   (a `FixCompile` starter must fail to compile, a `FindBug` starter must compile but fail tests,
//!   a `Refactor` starter must pass its tests but violate a style check).
//!
//! Quiz challenges are checked structurally (answer index in range).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use super::report::{RunReport, Verdict};
use super::runner::{Runner, RunnerConfig};
use super::toolchain::Toolchain;
use crate::data::{Challenge, ChallengeKind, ContentLibrary};

#[derive(Debug, Clone)]
pub struct Validation {
    pub id: String,
    pub kind: ChallengeKind,
    pub problems: Vec<String>,
    /// Short human description of how the starter code fails (for the log).
    pub starter_result: String,
    pub solution_result: String,
    pub millis: u128,
}

impl Validation {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

fn describe(report: &RunReport) -> String {
    let mut s = report.one_line();
    if let Some(code) = report.errors().find_map(|e| e.code.clone()) {
        s.push_str(&format!(" [{code}]"));
    }
    s
}

fn failure_detail(report: &RunReport) -> String {
    let mut parts = vec![report.one_line()];
    for e in report.errors().take(3) {
        parts.push(format!("  rustc: {}", e.rendered.trim()));
    }
    if !report.build_log.is_empty() {
        parts.push(format!("  cargo: {}", report.build_log));
    }
    if let Some(t) = &report.tests {
        for c in t.cases.iter().filter(|c| c.message.is_some()).take(3) {
            parts.push(format!("  test {}: {}", c.name, c.message.as_deref().unwrap_or("")));
        }
    }
    parts.extend(report.check_failures.iter().map(|c| format!("  check: {c}")));
    if let Some(n) = &report.note {
        parts.push(format!("  note: {n}"));
    }
    parts.join("\n")
}

/// Which verdicts are acceptable for the *starter* code, by kind.
fn starter_expectation(kind: ChallengeKind) -> (&'static [&'static str], &'static str) {
    match kind {
        ChallengeKind::FixCompile => (&["CompileError"], "must fail to compile"),
        ChallengeKind::FindBug => (&["TestsFailed", "Crashed", "Timeout"], "must compile but fail its tests"),
        ChallengeKind::Refactor => (&["CheckFailed"], "must pass the tests but violate a style check"),
        ChallengeKind::Performance => {
            (&["Timeout", "TestsFailed"], "must be too slow / fail its time assertions")
        }
        ChallengeKind::Complete | ChallengeKind::FromScratch => {
            (&["CompileError", "TestsFailed", "Crashed", "Timeout"], "must not pass")
        }
        ChallengeKind::PredictOutput | ChallengeKind::CodeReview => (&[], ""),
    }
}

fn verdict_name(v: &Verdict) -> &'static str {
    match v {
        Verdict::Passed => "Passed",
        Verdict::CompileError => "CompileError",
        Verdict::TestsFailed => "TestsFailed",
        Verdict::Timeout => "Timeout",
        Verdict::Crashed => "Crashed",
        Verdict::CheckFailed => "CheckFailed",
        Verdict::Cancelled => "Cancelled",
        Verdict::Unavailable(_) => "Unavailable",
    }
}

/// Prove a quiz answer with the real compiler where the quiz asks for it.
fn verify_quiz(runner: &Runner, c: &Challenge, q: &crate::data::Quiz, v: &mut Validation) {
    use crate::data::QuizVerify;
    let wrap = |option: &str| {
        let template =
            if q.wrapper.trim().is_empty() { "fn main() {\n{CODE}\n}\n" } else { q.wrapper.as_str() };
        template.replace("{CODE}", option)
    };
    match q.verify {
        QuizVerify::None => {}
        QuizVerify::RunOutput => {
            let result = runner.check_snippet(&q.code, true);
            if !result.compiled {
                v.problems.push(format!("quiz program does not compile:\n{}", result.diagnostics.trim()));
                return;
            }
            let got = result.stdout.trim().replace("\r\n", "\n");
            if got != c.solution.trim() {
                v.problems
                    .push(format!("the program prints:\n{got}\nbut `solution` says:\n{}", c.solution.trim()));
            }
            if q.options[q.correct].trim() != c.solution.trim() {
                v.problems.push("the correct option text must equal `solution`".into());
            }
            v.solution_result = "program output verified".into();
        }
        QuizVerify::OnlyCorrectFailsToCompile | QuizVerify::OnlyCorrectCompiles => {
            let want_fail = q.verify == QuizVerify::OnlyCorrectFailsToCompile;
            for (i, option) in q.options.iter().enumerate() {
                let compiled = runner.check_snippet(&wrap(option), false).compiled;
                let should_compile = if i == q.correct { !want_fail } else { want_fail };
                if compiled != should_compile {
                    v.problems.push(format!(
                        "option {} {} but should {}",
                        (b'A' + i as u8) as char,
                        if compiled { "compiles" } else { "does not compile" },
                        if should_compile { "compile" } else { "fail to compile" }
                    ));
                }
            }
            v.solution_result = "snippets verified with rustc".into();
        }
    }
}

pub fn validate_challenge(runner: &Runner, c: &Challenge) -> Validation {
    let started = Instant::now();
    let mut v = Validation {
        id: c.id.clone(),
        kind: c.kind,
        problems: Vec::new(),
        starter_result: String::new(),
        solution_result: String::new(),
        millis: 0,
    };

    if c.is_quiz() {
        match &c.quiz {
            Some(q) if q.correct < q.options.len() && q.options.len() >= 2 => {
                v.solution_result = format!("answer #{} of {}", q.correct + 1, q.options.len());
                verify_quiz(runner, c, q, &mut v);
            }
            _ => v.problems.push("quiz payload missing or `correct` out of range".into()),
        }
        v.millis = started.elapsed().as_millis();
        return v;
    }

    let never = AtomicBool::new(false);

    let solution = runner.run(c, &c.solution, &never, &|_| {});
    v.solution_result = describe(&solution);
    if !solution.verdict.is_pass() {
        v.problems.push(format!("reference solution does not pass: {}", failure_detail(&solution)));
    }

    let starter = runner.run(c, &c.starter_code, &never, &|_| {});
    v.starter_result = describe(&starter);
    let (allowed, why) = starter_expectation(c.kind);
    if starter.verdict.is_pass() {
        v.problems.push("starter code already passes — the challenge is trivially solved".into());
    } else if !allowed.contains(&verdict_name(&starter.verdict)) {
        v.problems.push(format!(
            "{:?} starter {why}, but its verdict is {}: {}",
            c.kind,
            verdict_name(&starter.verdict),
            failure_detail(&starter)
        ));
    }

    if c.solution.trim() == c.starter_code.trim() {
        v.problems.push("solution is identical to the starter code".into());
    }
    v.millis = started.elapsed().as_millis();
    v
}

pub struct ValidateOptions {
    pub workers: usize,
    /// Only validate challenges whose id contains this text.
    pub filter: Option<String>,
    pub config: RunnerConfig,
}

/// Validate every matching challenge using `workers` parallel sandboxes.
pub fn validate_all(
    content: &ContentLibrary,
    toolchain: &Toolchain,
    sandbox_root: &Path,
    options: &ValidateOptions,
    on_done: &(dyn Fn(&Validation) + Sync),
) -> Vec<Validation> {
    let todo: Vec<&Challenge> = content
        .challenges
        .iter()
        .filter(|c| options.filter.as_deref().is_none_or(|f| c.id.contains(f)))
        .collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Validation>> = Mutex::new(Vec::new());
    let workers = options.workers.clamp(1, todo.len().max(1));

    std::thread::scope(|scope| {
        for worker in 0..workers {
            let runner = Runner::new(toolchain.clone(), sandbox_root, worker, options.config.clone());
            let (next, results, todo) = (&next, &results, &todo);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(c) = todo.get(i) else {
                    break;
                };
                let validation = validate_challenge(&runner, c);
                on_done(&validation);
                results.lock().unwrap_or_else(|p| p.into_inner()).push(validation);
            });
        }
    });

    let mut all = results.into_inner().unwrap_or_else(|p| p.into_inner());
    all.sort_by(|a, b| a.id.cmp(&b.id));
    all
}

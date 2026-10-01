//! End-to-end tests of the sandbox runner. These invoke the real `cargo`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use std::time::Duration;

use studio_core::challenges::testparse::TestStatus;
use studio_core::challenges::{Level, Runner, RunnerConfig, Toolchain, Verdict};
use studio_core::data::Check;

fn root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("rst_runner_tests_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    })
}

fn runner(worker: usize, test_secs: u64) -> Runner {
    let toolchain = Toolchain::detect().expect("cargo is required to run these tests");
    let config = RunnerConfig {
        compile_timeout: Duration::from_secs(180),
        test_timeout: Duration::from_secs(test_secs),
        output_limit: 64 * 1024,
    };
    Runner::new(toolchain, root(), worker, config)
}

fn run(r: &Runner, code: &str, tests: &str, checks: &[Check]) -> studio_core::challenges::RunReport {
    r.run_code(code, tests, checks, None, &AtomicBool::new(false), &|_| {})
}

const ADD_TESTS: &str = r#"
#[test]
fn adds() { assert_eq!(add(2, 3), 5); }
#[test]
fn adds_negative() { assert_eq!(add(-2, -3), -5); }
"#;

#[test]
fn correct_code_passes() {
    let r = runner(1, 20);
    let rep = run(&r, "pub fn add(a: i32, b: i32) -> i32 { a + b }", ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::Passed, "{rep:#?}");
    let t = rep.tests.unwrap();
    assert_eq!((t.passed(), t.failed()), (2, 0));
    assert!(t.finished);
}

#[test]
fn compile_error_is_parsed_with_code_and_line() {
    let r = runner(2, 20);
    let code = "pub fn add(a: i32, b: i32) -> i32 {\n    let s = String::from(\"x\");\n    let t = s;\n    println!(\"{}\", s);\n    a + b\n}\n";
    let rep = run(&r, code, ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::CompileError);
    let e = rep.errors().next().expect("an error diagnostic");
    assert_eq!(e.code.as_deref(), Some("E0382"));
    assert_eq!(e.line, Some(4), "borrow of moved value is reported on line 4");
    assert!(!e.rendered.is_empty());
    assert!(rep.tests.is_none(), "tests must not run when compilation fails");
}

#[test]
fn failing_assertions_report_left_and_right() {
    let r = runner(3, 20);
    let rep = run(&r, "pub fn add(a: i32, b: i32) -> i32 { a - b }", ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::TestsFailed, "{rep:#?}");
    let t = rep.tests.unwrap();
    assert_eq!(t.failed(), 2);
    let adds = t.cases.iter().find(|c| c.name == "adds").unwrap();
    assert_eq!(adds.status, TestStatus::Failed);
    let msg = adds.message.as_deref().unwrap();
    assert!(msg.contains("left: -1") && msg.contains("right: 5"), "{msg}");
}

#[test]
fn infinite_loop_is_killed_at_the_timeout() {
    let r = runner(4, 3);
    let code = "pub fn add(a: i32, b: i32) -> i32 { loop {} }";
    let started = std::time::Instant::now();
    let rep = run(&r, code, ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::Timeout, "{rep:#?}");
    assert!(rep.note.as_deref().unwrap().contains("3 seconds"));
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[test]
fn stack_overflow_is_reported_as_a_crash() {
    let r = runner(5, 20);
    let code = "pub fn add(a: i32, b: i32) -> i32 { add(a, b) + 1 }";
    let rep = run(&r, code, ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::Crashed, "{rep:#?}");
    assert!(rep.note.as_deref().unwrap().to_lowercase().contains("stack"), "{:?}", rep.note);
}

#[test]
fn output_flood_is_capped() {
    let r = runner(6, 20);
    // Raw writes bypass libtest's output capture, like a hostile (or just noisy) program would.
    let code = "use std::io::Write;\npub fn add(a: i32, b: i32) -> i32 {\n    let mut e = std::io::stderr();\n    for _ in 0..200_000 { let _ = e.write_all(b\"spam spam spam spam spam spam\\n\"); }\n    a + b\n}";
    let tests = "#[test]\nfn adds() { assert_eq!(add(2, 3), 5); }";
    let rep = run(&r, code, tests, &[]);
    assert!(rep.test_output.len() <= 2 * 64 * 1024 + 1024, "got {} bytes", rep.test_output.len());
    assert!(rep.output_truncated);
}

#[test]
fn style_checks_apply_after_tests_pass() {
    let r = runner(7, 20);
    let checks = [Check::ForbidText { text: ".clone()".into(), message: "no clones allowed".into() }];
    let code = "pub fn add(a: i32, b: i32) -> i32 { let c = a.clone(); c + b }";
    let rep = run(&r, code, ADD_TESTS, &checks);
    assert_eq!(rep.verdict, Verdict::CheckFailed);
    assert_eq!(rep.check_failures, vec!["no clones allowed".to_string()]);
    let ok = run(&r, "pub fn add(a: i32, b: i32) -> i32 { a + b }", ADD_TESTS, &checks);
    assert_eq!(ok.verdict, Verdict::Passed);
}

#[test]
fn wrong_signature_is_blamed_on_the_hidden_tests_not_the_player() {
    let r = runner(8, 20);
    let rep = run(&r, "pub fn add(a: i32) -> i32 { a }", ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::CompileError);
    let e = rep.errors().next().unwrap();
    assert!(e.in_hidden_tests, "{e:#?}");
    assert_eq!(e.line, None);
}

#[test]
fn warnings_are_reported_but_do_not_fail() {
    let r = runner(9, 20);
    let code = "pub fn add(a: i32, b: i32) -> i32 { let unused = 1; a + b }";
    let rep = run(&r, code, ADD_TESTS, &[]);
    assert_eq!(rep.verdict, Verdict::Passed);
    assert!(rep.warnings().any(|w| w.message.contains("unused variable")));
    assert!(rep.diagnostics.iter().all(|d| d.level != Level::Error));
}

#[test]
fn cancel_before_start_returns_cancelled() {
    let r = runner(10, 20);
    let cancel = AtomicBool::new(true);
    let rep =
        r.run_code("pub fn add(a: i32, b: i32) -> i32 { a + b }", ADD_TESTS, &[], None, &cancel, &|_| {});
    assert_eq!(rep.verdict, Verdict::Cancelled);
}

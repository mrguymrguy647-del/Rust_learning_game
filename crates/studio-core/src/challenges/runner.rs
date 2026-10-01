//! The compile-and-test pipeline: assemble → `cargo test --no-run` → run the test binary → checks.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::checks;
use super::diagnostics::{parse_cargo_json, Level};
use super::process::{run_limited, Limits, ProcStatus};
use super::report::{Phase, RunReport, Verdict};
use super::sandbox::{assemble, Sandbox};
use super::testparse::{explain_panic, parse_libtest};
use super::toolchain::Toolchain;
use crate::data::{Challenge, Check};
use crate::settings::Settings;

/// Environment variables the child processes may inherit. Everything else is dropped.
const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SystemRoot",
    "SYSTEMROOT",
    "SystemDrive",
    "APPDATA",
    "LOCALAPPDATA",
    "LANG",
    "LC_ALL",
    "USER",
    "LOGNAME",
];

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub compile_timeout: Duration,
    pub test_timeout: Duration,
    /// Max bytes of output kept from any process.
    pub output_limit: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        RunnerConfig {
            compile_timeout: Duration::from_secs(120),
            test_timeout: Duration::from_secs(20),
            output_limit: 256 * 1024,
        }
    }
}

impl RunnerConfig {
    pub fn from_settings(s: &Settings) -> RunnerConfig {
        RunnerConfig {
            compile_timeout: Duration::from_secs(s.compile_timeout_secs),
            test_timeout: Duration::from_secs(s.sandbox_timeout_secs),
            ..RunnerConfig::default()
        }
    }
}

/// Result of compiling (and maybe running) a standalone snippet.
#[derive(Debug, Clone)]
pub struct SnippetResult {
    pub compiled: bool,
    pub stdout: String,
    pub diagnostics: String,
}

pub struct Runner {
    toolchain: Toolchain,
    sandbox: Sandbox,
    config: Mutex<RunnerConfig>,
    /// One submission at a time per sandbox project.
    lock: Mutex<()>,
}

impl Runner {
    pub fn new(toolchain: Toolchain, sandbox_root: &Path, worker: usize, config: RunnerConfig) -> Runner {
        Runner {
            toolchain,
            sandbox: Sandbox::new(sandbox_root, worker),
            config: Mutex::new(config),
            lock: Mutex::new(()),
        }
    }

    pub fn config(&self) -> RunnerConfig {
        self.config.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Apply new limits (e.g. the player changed the timeout in settings). Takes effect on the next run.
    pub fn set_config(&self, config: RunnerConfig) {
        *self.config.lock().unwrap_or_else(|p| p.into_inner()) = config;
    }

    pub fn toolchain(&self) -> &Toolchain {
        &self.toolchain
    }

    pub fn sandbox(&self) -> &Sandbox {
        &self.sandbox
    }

    fn command(&self, program: impl Into<OsString>) -> Command {
        let mut cmd = Command::new(program.into());
        cmd.env_clear();
        for key in ENV_ALLOWLIST {
            if let Some(v) = env::var_os(key) {
                cmd.env(key, v);
            }
        }
        // cargo must find its sibling `rustc` even when only an absolute cargo path is known.
        if let Some(bin) = self.toolchain.bin_dir() {
            let mut paths: Vec<PathBuf> = vec![bin];
            if let Some(existing) = env::var_os("PATH") {
                paths.extend(env::split_paths(&existing));
            }
            if let Ok(joined) = env::join_paths(paths) {
                cmd.env("PATH", joined);
            }
        }
        cmd.env("CARGO_TARGET_DIR", self.sandbox.target_dir())
            .env("CARGO_TERM_COLOR", "never")
            .env("RUST_BACKTRACE", "0")
            .env("CARGO_INCREMENTAL", "1")
            .current_dir(self.sandbox.project_dir());
        cmd
    }

    /// Compile a standalone Rust file with `rustc` and optionally run it (used to verify quizzes).
    pub fn check_snippet(&self, source: &str, run: bool) -> SnippetResult {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let config = self.config();
        let dir = self.sandbox.snippet_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return SnippetResult {
                compiled: false,
                stdout: String::new(),
                diagnostics: format!("cannot create {}: {e}", dir.display()),
            };
        }
        let src = dir.join("snippet.rs");
        let exe = dir.join(if cfg!(windows) { "snippet.exe" } else { "snippet" });
        if let Err(e) = std::fs::write(&src, source) {
            return SnippetResult { compiled: false, stdout: String::new(), diagnostics: e.to_string() };
        }
        let _ = std::fs::remove_file(&exe);
        let rustc = self.toolchain.rustc_path();
        let mut cmd = self.command(rustc);
        cmd.args(["--edition", "2021", "--color", "never", "-A", "warnings", "-o"])
            .arg(&exe)
            .arg(&src)
            .current_dir(&dir);
        let never = AtomicBool::new(false);
        let compiled = run_limited(cmd, &Limits::new(config.compile_timeout, config.output_limit), &never);
        if !compiled.success() {
            return SnippetResult {
                compiled: false,
                stdout: String::new(),
                diagnostics: format!("{}{}", compiled.stdout, compiled.stderr),
            };
        }
        if !run {
            return SnippetResult { compiled: true, stdout: String::new(), diagnostics: String::new() };
        }
        let run_cmd = self.command(&exe);
        let ran =
            run_limited(run_cmd, &Limits::for_player_code(config.test_timeout, config.output_limit), &never);
        SnippetResult { compiled: true, stdout: ran.stdout, diagnostics: ran.stderr }
    }

    /// Build the (tiny) project once so the first real check is quick.
    pub fn warm_up(&self) -> RunReport {
        let challenge_tests = "#[test]\nfn warm_up() { assert_eq!(1 + 1, 2); }";
        self.run_code(
            "pub fn warm() -> i32 { 2 }",
            challenge_tests,
            &[],
            None,
            &AtomicBool::new(false),
            &|_| {},
        )
    }

    /// Grade `player_code` against `challenge`.
    pub fn run(
        &self,
        challenge: &Challenge,
        player_code: &str,
        cancel: &AtomicBool,
        on_phase: &dyn Fn(Phase),
    ) -> RunReport {
        let timeout = challenge.test_timeout_secs.map(Duration::from_secs);
        self.run_code(player_code, &challenge.hidden_tests, &challenge.checks, timeout, cancel, on_phase)
    }

    pub fn run_code(
        &self,
        player_code: &str,
        hidden_tests: &str,
        checks_to_run: &[Check],
        test_timeout_override: Option<Duration>,
        cancel: &AtomicBool,
        on_phase: &dyn Fn(Phase),
    ) -> RunReport {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let config = self.config();

        on_phase(Phase::Preparing);
        if let Err(e) = self.sandbox.prepare(&[]) {
            return RunReport::empty(Verdict::Unavailable(format!("cannot create sandbox: {e}")));
        }
        let assembled = assemble(player_code, hidden_tests);
        if let Err(e) = self.sandbox.write_source(&assembled.source) {
            return RunReport::empty(Verdict::Unavailable(format!("cannot write sandbox: {e}")));
        }

        // ---- compile ---------------------------------------------------------------------
        on_phase(Phase::Compiling);
        let mut cmd = self.command(&self.toolchain.cargo);
        cmd.args(["test", "--no-run", "--message-format=json", "--color", "never", "--offline"])
            .arg("--manifest-path")
            .arg(self.sandbox.manifest_path());
        let compile_limits = Limits::new(config.compile_timeout, config.output_limit * 8);
        let compiled = run_limited(cmd, &compile_limits, cancel);
        let compile_ms = compiled.elapsed.as_millis() as u64;

        let mut report = RunReport::empty(Verdict::Passed);
        report.compile_ms = compile_ms;
        match &compiled.status {
            ProcStatus::Cancelled => {
                report.verdict = Verdict::Cancelled;
                return report;
            }
            ProcStatus::TimedOut => {
                report.verdict = Verdict::Timeout;
                report.note = Some(format!(
                    "Compilation took longer than {} seconds and was stopped.",
                    config.compile_timeout.as_secs()
                ));
                return report;
            }
            ProcStatus::SpawnFailed(why) => {
                report.verdict = Verdict::Unavailable(format!("could not start cargo: {why}"));
                return report;
            }
            _ => {}
        }

        let parsed = parse_cargo_json(&compiled.stdout, assembled.player_lines);
        report.diagnostics = parsed.diagnostics;
        report.check_failures = checks::run_checks(player_code, checks_to_run);
        let compile_ok = compiled.success() && parsed.build_success != Some(false);
        if !compile_ok {
            report.verdict = Verdict::CompileError;
            let mut log = parsed.other_output.join("\n");
            if !compiled.stderr.trim().is_empty() && report.errors().next().is_none() {
                log.push('\n');
                log.push_str(compiled.stderr.trim());
            }
            report.build_log = log.trim().to_string();
            // Cargo failed before rustc ran (e.g. a toolchain problem): say so instead of "0 errors".
            if report.errors().next().is_none() && report.build_log.is_empty() {
                report.build_log = "cargo failed without any compiler message".to_string();
            }
            return report;
        }

        let Some(exe) = parsed.test_executable else {
            report.verdict = Verdict::Unavailable("cargo did not report a test executable".into());
            report.build_log = compiled.stderr;
            return report;
        };

        // ---- run the tests ---------------------------------------------------------------
        on_phase(Phase::Testing);
        let timeout = test_timeout_override.map_or(config.test_timeout, |t| t.max(config.test_timeout));
        let mut test_cmd = self.command(exe);
        test_cmd.args(["--test-threads=1", "--color", "never"]);
        let limits = Limits::for_player_code(timeout, config.output_limit);
        let ran = run_limited(test_cmd, &limits, cancel);
        report.test_ms = ran.elapsed.as_millis() as u64;
        report.output_truncated = ran.stdout_truncated || ran.stderr_truncated;
        report.test_output = format!("{}{}", ran.stdout, ran.stderr);

        let parse = parse_libtest(&ran.stdout, &ran.stderr);
        let lost_tests = parse.cases.is_empty();
        let failed_tests = parse.failed() > 0;
        report.tests = Some(parse);

        report.verdict = match &ran.status {
            ProcStatus::Cancelled => Verdict::Cancelled,
            ProcStatus::TimedOut => {
                report.note = Some(format!(
                    "Your code ran for more than {} seconds and was stopped. An infinite loop, a deadlock, or an \
                     algorithm that is too slow for the test inputs are the usual causes.",
                    timeout.as_secs()
                ));
                Verdict::Timeout
            }
            ProcStatus::SpawnFailed(why) => Verdict::Unavailable(format!("could not start the tests: {why}")),
            ProcStatus::Exited(0) if !lost_tests && !failed_tests => Verdict::Passed,
            ProcStatus::Exited(0) => Verdict::TestsFailed,
            ProcStatus::Exited(101) if failed_tests => Verdict::TestsFailed,
            ProcStatus::Exited(_) | ProcStatus::Signaled(_) => {
                report.note = Some(crash_note(&ran.status, &report.test_output));
                Verdict::Crashed
            }
        };
        if let (Verdict::TestsFailed, Some(t)) = (&report.verdict, &mut report.tests) {
            // A panic explanation is added to the message so the UI can show it verbatim.
            for case in t.cases.iter_mut().filter(|c| c.message.is_some()) {
                if let Some(extra) = case.message.as_deref().and_then(explain_panic) {
                    if let Some(m) = case.message.as_mut() {
                        m.push_str("\n» ");
                        m.push_str(extra);
                    }
                }
            }
        }

        // ---- style checks ----------------------------------------------------------------
        if report.verdict == Verdict::Passed {
            on_phase(Phase::Checking);
            if !report.check_failures.is_empty() {
                report.verdict = Verdict::CheckFailed;
            }
        }
        report
    }
}

fn crash_note(status: &ProcStatus, output: &str) -> String {
    if output.contains("overflowed its stack") {
        return "Stack overflow: most likely infinite (or very deep) recursion.".into();
    }
    if output.contains("memory allocation of") {
        return "The program tried to allocate more memory than the sandbox allows.".into();
    }
    match status {
        ProcStatus::Signaled(9) => "The process was killed (probably out of memory).".into(),
        ProcStatus::Signaled(24) => "The process used too much CPU time and was stopped.".into(),
        ProcStatus::Signaled(11) => "The program crashed with a segmentation fault.".into(),
        ProcStatus::Signaled(6) => "The program aborted.".into(),
        ProcStatus::Signaled(n) => format!("The program was terminated by signal {n}."),
        ProcStatus::Exited(code) => format!("The test program exited unexpectedly with code {code}."),
        _ => "The program stopped unexpectedly.".into(),
    }
}

// ---- background execution for the UI -------------------------------------------------------

pub enum RunEvent {
    Phase(Phase),
    Done(Box<RunReport>),
}

/// A submission running on its own thread. Poll it from the UI loop.
pub struct RunHandle {
    rx: mpsc::Receiver<RunEvent>,
    cancel: Arc<AtomicBool>,
    finished: bool,
}

impl RunHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain pending events. If the worker thread died, a failed report is synthesized.
    pub fn poll(&mut self) -> Vec<RunEvent> {
        let mut events = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(ev) => {
                    if matches!(ev, RunEvent::Done(_)) {
                        self.finished = true;
                    }
                    events.push(ev);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !self.finished {
                        self.finished = true;
                        events.push(RunEvent::Done(Box::new(RunReport::empty(Verdict::Unavailable(
                            "the checking thread crashed unexpectedly".into(),
                        )))));
                    }
                    break;
                }
            }
        }
        events
    }
}

/// Start grading on a background thread. `wake` is called after every event (use it to ask the
/// UI for a repaint).
pub fn spawn_run(
    runner: Arc<Runner>,
    challenge: Challenge,
    code: String,
    wake: impl Fn() + Send + 'static,
) -> RunHandle {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    thread::spawn(move || {
        let tx_phase = tx.clone();
        let report = runner.run(&challenge, &code, &cancel_for_thread, &|p| {
            let _ = tx_phase.send(RunEvent::Phase(p));
            wake();
        });
        let _ = tx.send(RunEvent::Done(Box::new(report)));
        wake();
    });
    RunHandle { rx, cancel, finished: false }
}

/// Count of error-level diagnostics (helper for summaries).
pub fn error_count(report: &RunReport) -> usize {
    report.diagnostics.iter().filter(|d| d.level == Level::Error).count()
}

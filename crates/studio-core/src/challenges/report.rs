//! The outcome of compiling and testing one submission.

use super::diagnostics::{Diagnostic, Level};
use super::testparse::TestParse;

/// Pipeline stages, reported to the UI as progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Compiling,
    Testing,
    Checking,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Preparing => "Preparing sandbox…",
            Phase::Compiling => "Compiling with cargo…",
            Phase::Testing => "Running hidden tests…",
            Phase::Checking => "Checking code style…",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Compiles, all tests pass, all checks pass.
    Passed,
    CompileError,
    TestsFailed,
    /// Compilation or the tests exceeded the time limit.
    Timeout,
    /// The test binary died (stack overflow, abort, out of memory, …).
    Crashed,
    /// Tests pass but a style requirement (e.g. "no unwrap") is violated.
    CheckFailed,
    Cancelled,
    /// No usable toolchain / the sandbox could not run.
    Unavailable(String),
}

impl Verdict {
    pub fn is_pass(&self) -> bool {
        *self == Verdict::Passed
    }
}

#[derive(Debug, Clone)]
pub struct RunReport {
    pub verdict: Verdict,
    pub diagnostics: Vec<Diagnostic>,
    /// Cargo output that was not a compiler message (manifest errors, network problems, …).
    pub build_log: String,
    pub tests: Option<TestParse>,
    /// Raw test-binary output (stdout then stderr), capped.
    pub test_output: String,
    pub output_truncated: bool,
    pub check_failures: Vec<String>,
    /// Extra explanation for crashes / timeouts.
    pub note: Option<String>,
    pub compile_ms: u64,
    pub test_ms: u64,
}

impl RunReport {
    pub fn empty(verdict: Verdict) -> RunReport {
        RunReport {
            verdict,
            diagnostics: Vec::new(),
            build_log: String::new(),
            tests: None,
            test_output: String::new(),
            output_truncated: false,
            check_failures: Vec::new(),
            note: None,
            compile_ms: 0,
            test_ms: 0,
        }
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.level == Level::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.level == Level::Warning)
    }

    pub fn one_line(&self) -> String {
        match &self.verdict {
            Verdict::Passed => "All tests passed".to_string(),
            Verdict::CompileError => format!("{} compile error(s)", self.errors().count()),
            Verdict::TestsFailed => match &self.tests {
                Some(t) => format!("{} of {} tests failed", t.failed(), t.passed() + t.failed()),
                None => "tests failed".to_string(),
            },
            Verdict::Timeout => "Timed out".to_string(),
            Verdict::Crashed => "The program crashed".to_string(),
            Verdict::CheckFailed => format!("{} style requirement(s) not met", self.check_failures.len()),
            Verdict::Cancelled => "Cancelled".to_string(),
            Verdict::Unavailable(why) => format!("Cannot run: {why}"),
        }
    }
}

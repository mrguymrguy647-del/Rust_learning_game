//! Real-Rust challenge checking: sandbox project, cargo runner, diagnostics, grading.

pub mod checks;
pub mod diagnostics;
pub mod process;
pub mod report;
pub mod runner;
pub mod sandbox;
pub mod testparse;
pub mod toolchain;
pub mod validate;

pub use diagnostics::{Diagnostic, Level};
pub use report::{Phase, RunReport, Verdict};
pub use runner::{spawn_run, RunEvent, RunHandle, Runner, RunnerConfig};
pub use toolchain::{Toolchain, ToolchainError, INSTALL_INSTRUCTIONS};

/// Grade a multiple-choice answer.
pub fn grade_quiz(challenge: &crate::data::Challenge, selected: usize) -> bool {
    challenge.quiz.as_ref().is_some_and(|q| q.correct == selected)
}

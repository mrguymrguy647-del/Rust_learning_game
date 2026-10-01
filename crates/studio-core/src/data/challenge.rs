//! Challenge schema. Every challenge is a RON value loaded from `content/challenges/*.ron`.

use serde::{Deserialize, Serialize};

/// The eight challenge flavours the game supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChallengeKind {
    /// Starter code does not compile; the player fixes it.
    FixCompile,
    /// Fill in function bodies so the hidden tests pass.
    Complete,
    /// Write the whole thing from a specification.
    FromScratch,
    /// Code compiles but a logic bug makes tests fail.
    FindBug,
    /// Multiple choice: what does this program print? (no compiling)
    PredictOutput,
    /// Rewrite into idiomatic Rust; graded by tests plus static checks.
    Refactor,
    /// Must stay within a time limit on large inputs.
    Performance,
    /// Multiple choice: which snippet is buggy / unsafe? (no compiling)
    CodeReview,
}

impl ChallengeKind {
    pub const ALL: [ChallengeKind; 8] = [
        ChallengeKind::FixCompile,
        ChallengeKind::Complete,
        ChallengeKind::FromScratch,
        ChallengeKind::FindBug,
        ChallengeKind::PredictOutput,
        ChallengeKind::Refactor,
        ChallengeKind::Performance,
        ChallengeKind::CodeReview,
    ];

    /// Quiz kinds are answered by picking an option and never touch the compiler.
    pub fn is_quiz(self) -> bool {
        matches!(self, ChallengeKind::PredictOutput | ChallengeKind::CodeReview)
    }

    pub fn label(self) -> &'static str {
        match self {
            ChallengeKind::FixCompile => "Fix the compile error",
            ChallengeKind::Complete => "Complete the function",
            ChallengeKind::FromScratch => "Write from scratch",
            ChallengeKind::FindBug => "Find the logic bug",
            ChallengeKind::PredictOutput => "Predict the output",
            ChallengeKind::Refactor => "Refactor to idiomatic Rust",
            ChallengeKind::Performance => "Performance challenge",
            ChallengeKind::CodeReview => "Code review",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ChallengeKind::FixCompile => "🛠",
            ChallengeKind::Complete => "✏",
            ChallengeKind::FromScratch => "📄",
            ChallengeKind::FindBug => "🐞",
            ChallengeKind::PredictOutput => "🔮",
            ChallengeKind::Refactor => "✨",
            ChallengeKind::Performance => "⏱",
            ChallengeKind::CodeReview => "🔍",
        }
    }
}

/// A simple static check applied to the player's code (comments and string literals are
/// blanked out first, so a forbidden word inside a comment does not count).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Check {
    /// The identifier/keyword must not appear (whole-word match), e.g. `unwrap`, `for`.
    ForbidWord { word: String, message: String },
    /// The identifier/keyword must appear (whole-word match).
    RequireWord { word: String, message: String },
    /// This text must not appear (whitespace-insensitive substring), e.g. `.clone()`.
    ForbidText { text: String, message: String },
    /// This text must appear (whitespace-insensitive substring).
    RequireText { text: String, message: String },
}

impl Check {
    pub fn message(&self) -> &str {
        match self {
            Check::ForbidWord { message, .. }
            | Check::RequireWord { message, .. }
            | Check::ForbidText { message, .. }
            | Check::RequireText { message, .. } => message,
        }
    }
}

/// How the validator proves a quiz's answer is right using the real compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum QuizVerify {
    /// Structural check only (reasoning questions).
    #[default]
    None,
    /// `code` is a complete program; compiling and running it must print exactly `solution`.
    RunOutput,
    /// Each option, placed into `wrapper`, is compiled: exactly the correct option must FAIL.
    OnlyCorrectFailsToCompile,
    /// Each option, placed into `wrapper`, is compiled: exactly the correct option must COMPILE.
    OnlyCorrectCompiles,
}

/// Multiple-choice payload used by `PredictOutput` and `CodeReview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quiz {
    pub question: String,
    /// Optional code shown above the options (the program to predict).
    #[serde(default)]
    pub code: String,
    pub options: Vec<String>,
    /// Index into `options` of the right answer.
    pub correct: usize,
    /// Render each option as a code block (code review snippets).
    #[serde(default)]
    pub options_are_code: bool,
    #[serde(default)]
    pub verify: QuizVerify,
    /// Source template for `OnlyCorrect…` verification; `{CODE}` is replaced by an option.
    /// Defaults to `fn main() {\n{CODE}\n}`.
    #[serde(default)]
    pub wrapper: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookLink {
    pub title: String,
    pub url: String,
}

/// What solving a challenge pays out. All-zero means "derive from difficulty".
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rewards {
    /// Development progress points added to the current project (project context only).
    pub dev_points: u32,
    /// Quality bonus added to the current project (first-try solves multiply this).
    pub quality: f32,
    /// Engine research points.
    pub research: u32,
    /// Player XP.
    pub xp: u32,
}

impl Rewards {
    pub fn is_zero(&self) -> bool {
        self.dev_points == 0 && self.quality == 0.0 && self.research == 0 && self.xp == 0
    }

    /// Default rewards for a difficulty level (1..=5).
    pub fn for_difficulty(difficulty: u8) -> Rewards {
        let d = difficulty.clamp(1, 5) as u32;
        Rewards { dev_points: 20 * d, quality: 2.0 + 1.5 * d as f32, research: 10 * d, xp: 15 * d + 5 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    /// Id of the curriculum topic (see `game/topics.ron`).
    pub topic: String,
    pub kind: ChallengeKind,
    /// 1 (easy) ..= 5 (expert).
    pub difficulty: u8,
    pub title: String,
    /// Narrative framing shown first ("Your physics engine crashes when…").
    pub story: String,
    /// What exactly the player must do.
    pub task: String,
    /// Function signatures/items the player must provide (shown in the editor header).
    #[serde(default)]
    pub signatures: Vec<String>,
    #[serde(default)]
    pub starter_code: String,
    /// Test functions (and any helper `use` lines). Injected into
    /// `#[cfg(test)] mod hidden_tests { use super::*; … }`.
    #[serde(default)]
    pub hidden_tests: String,
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Three tiers: concept nudge, specific direction, partial code.
    #[serde(default)]
    pub hints: Vec<String>,
    #[serde(default)]
    pub solution: String,
    /// "What you learned" shown after solving.
    pub explanation: String,
    #[serde(default)]
    pub book_links: Vec<BookLink>,
    #[serde(default)]
    pub rewards: Rewards,
    /// Challenge ids that must be solved first.
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// Free-form tags used to match challenges to engine modules / project focus
    /// (e.g. `"physics"`, `"audio"`, `"networking"`).
    #[serde(default)]
    pub engine_tags: Vec<String>,
    #[serde(default)]
    pub quiz: Option<Quiz>,
    /// Override of the global sandbox timeout for this challenge (seconds).
    #[serde(default)]
    pub test_timeout_secs: Option<u64>,
}

impl Challenge {
    /// Effective rewards (explicit if set, otherwise derived from difficulty).
    pub fn effective_rewards(&self) -> Rewards {
        if self.rewards.is_zero() {
            Rewards::for_difficulty(self.difficulty)
        } else {
            self.rewards
        }
    }

    /// Money cost of hint `tier` (0-based: 0, 1, 2) at the given multiplier.
    pub fn hint_cost(&self, tier: usize, multiplier: f32) -> i64 {
        let base = 60.0 * self.difficulty.clamp(1, 5) as f32;
        let tier_mult = [1.0, 2.0, 4.0].get(tier).copied().unwrap_or(4.0);
        (base * tier_mult * multiplier).round() as i64
    }

    /// Cost of hiring a contractor to solve this challenge.
    pub fn contractor_cost(&self, multiplier: f32) -> i64 {
        (900.0 * self.difficulty.clamp(1, 5) as f32 * multiplier).round() as i64
    }

    pub fn is_quiz(&self) -> bool {
        self.kind.is_quiz()
    }
}

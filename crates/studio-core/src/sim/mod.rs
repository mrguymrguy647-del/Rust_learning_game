//! The tycoon simulation. Pure logic: no UI, no I/O, deterministic given the RNG state.

pub mod challenge_flow;
pub mod economy;
pub mod engine;
pub mod library;
pub mod model;
pub mod notify;
pub mod progress;
pub mod project;
pub mod quality;
pub mod reviews;
pub mod rng;
pub mod sales;
pub mod staff;
pub mod state;
pub mod tick;
pub mod time;

pub use challenge_flow::{Attempt, ChallengeContext, HintPayment, RewardSummary};
pub use library::{ReleasedGame, Review};
pub use model::{Audience, Category, Phase, ProjectSize, Weights};
pub use progress::{Availability, PlayerProgress, SolveRecord};
pub use project::{Project, ProjectConfig};
pub use rng::GameRng;
pub use staff::{Skill, Staff};
pub use state::{GameState, Studio};
pub use time::GameDate;

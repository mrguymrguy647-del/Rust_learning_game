//! The tycoon simulation. Pure logic: no UI, no I/O, deterministic given the RNG state.

pub mod challenge_flow;
pub mod progress;
pub mod rng;
pub mod state;
pub mod time;

pub use challenge_flow::{Attempt, ChallengeContext, HintPayment, RewardSummary};
pub use progress::{Availability, PlayerProgress, SolveRecord};
pub use rng::GameRng;
pub use state::{GameState, Studio};
pub use time::GameDate;

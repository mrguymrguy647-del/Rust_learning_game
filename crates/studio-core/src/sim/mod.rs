//! The tycoon simulation. Pure logic: no UI, no I/O, deterministic given the RNG state.

pub mod rng;
pub mod state;
pub mod time;

pub use rng::GameRng;
pub use state::{GameState, Studio};
pub use time::GameDate;

//! Rust Studio Tycoon core: content, challenge runner, simulation and persistence.
//!
//! Nothing in this crate depends on a UI toolkit, so every rule of the game is unit-testable.

pub mod challenges;
pub mod data;
pub mod fmt;
pub mod paths;
pub mod save;
pub mod settings;
pub mod sim;

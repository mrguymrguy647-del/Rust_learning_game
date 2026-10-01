//! Root of the serializable game state.

use serde::{Deserialize, Serialize};

use super::{GameDate, GameRng, PlayerProgress};
use crate::settings::Difficulty;

/// The player's company.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Studio {
    pub name: String,
    pub founder: String,
    /// Index into the content tier list.
    pub tier: usize,
    pub money: i64,
    /// Fans / reputation.
    pub reputation: i64,
    /// Engine research points earned from engine-lab challenges.
    pub research_points: i64,
}

impl Default for Studio {
    fn default() -> Self {
        Studio {
            name: "Untitled Studio".into(),
            founder: "You".into(),
            tier: 0,
            money: 15_000,
            reputation: 0,
            research_points: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameState {
    pub studio: Studio,
    pub date: GameDate,
    pub difficulty: Difficulty,
    pub rng: GameRng,
    pub progress: PlayerProgress,
    /// A challenge in progress (the clock is paused while this is set).
    pub pending_attempt: Option<super::Attempt>,
}

impl Default for GameState {
    fn default() -> Self {
        GameState {
            studio: Studio::default(),
            date: GameDate::default(),
            difficulty: Difficulty::Normal,
            rng: GameRng::from_seed(1),
            progress: PlayerProgress::default(),
            pending_attempt: None,
        }
    }
}

impl GameState {
    /// A fresh game. `seed` makes the world reproducible (tests); `None` uses OS entropy.
    pub fn new_game(studio: &str, founder: &str, difficulty: Difficulty, seed: Option<u64>) -> GameState {
        let rng = match seed {
            Some(s) => GameRng::from_seed(s),
            None => GameRng::from_entropy(),
        };
        let studio_name = studio.trim();
        let founder_name = founder.trim();
        GameState {
            studio: Studio {
                name: if studio_name.is_empty() { "Untitled Studio".into() } else { studio_name.into() },
                founder: if founder_name.is_empty() { "You".into() } else { founder_name.into() },
                ..Studio::default()
            },
            date: GameDate::default(),
            difficulty,
            rng,
            progress: PlayerProgress::default(),
            pending_attempt: None,
        }
    }

    /// Advance the calendar by `weeks` (full simulation arrives with the tycoon loop).
    pub fn advance_weeks(&mut self, weeks: u32) {
        self.date.advance(weeks);
    }
}

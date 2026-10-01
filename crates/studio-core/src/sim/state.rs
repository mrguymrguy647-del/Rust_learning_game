//! Root of the serializable game state.

use serde::{Deserialize, Serialize};

use super::economy::{FinanceBook, GameOver};
use super::engine::EngineState;
use super::library::ReleasedGame;
use super::notify::{NoteKind, Notification, MAX_FEED};
use super::project::Project;
use super::staff::Staff;
use super::{Attempt, GameDate, GameRng, PlayerProgress};
use crate::data::ContentLibrary;
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
    pub pending_attempt: Option<Attempt>,
    pub staff: Vec<Staff>,
    pub engine: EngineState,
    pub project: Option<Project>,
    pub games: Vec<ReleasedGame>,
    pub feed: Vec<Notification>,
    pub finances: FinanceBook,
    /// Consecutive weeks with negative cash.
    pub debt_weeks: u32,
    pub game_over: Option<GameOver>,
    next_entity_id: u32,
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
            staff: vec![Staff::founder(1, "You")],
            engine: EngineState::default(),
            project: None,
            games: Vec::new(),
            feed: Vec::new(),
            finances: FinanceBook::default(),
            debt_weeks: 0,
            game_over: None,
            next_entity_id: 2,
        }
    }
}

impl GameState {
    /// A fresh game. `seed` makes the world reproducible (tests); `None` uses OS entropy.
    pub fn new_game(
        content: &ContentLibrary,
        studio: &str,
        founder: &str,
        difficulty: Difficulty,
        seed: Option<u64>,
    ) -> GameState {
        let rng = match seed {
            Some(s) => GameRng::from_seed(s),
            None => GameRng::from_entropy(),
        };
        let studio_name = studio.trim();
        let founder_name = founder.trim();
        let founder_name = if founder_name.is_empty() { "You" } else { founder_name };
        let mut state = GameState {
            studio: Studio {
                name: if studio_name.is_empty() { "Untitled Studio".into() } else { studio_name.into() },
                founder: founder_name.into(),
                money: content.balance.start_money,
                ..Studio::default()
            },
            difficulty,
            rng,
            staff: vec![Staff::founder(1, founder_name)],
            engine: EngineState::initial(content),
            ..GameState::default()
        };
        state.note(
            NoteKind::Info,
            format!("{} is open for business. Time to make a game!", state.studio.name),
        );
        state
    }

    /// A fresh id for projects, staff and games.
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        id
    }

    pub fn note(&mut self, kind: NoteKind, text: impl Into<String>) {
        self.feed.push(Notification { week: self.date.week(), kind, text: text.into() });
        if self.feed.len() > MAX_FEED {
            self.feed.remove(0);
        }
    }

    pub fn founder(&self) -> Option<&Staff> {
        self.staff.iter().find(|s| s.is_founder)
    }

    /// Total money earned by all games.
    pub fn total_revenue(&self) -> i64 {
        self.games.iter().map(|g| g.revenue_total).sum()
    }
}

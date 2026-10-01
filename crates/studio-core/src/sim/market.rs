//! The living market: genre and theme trends, platform life cycles, competitor studios and the
//! yearly awards.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::notify::NoteKind;
use super::rng::GameRng;
use super::state::GameState;
use crate::data::ContentLibrary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompetitorState {
    pub id: String,
    pub next_release: u32,
    pub released: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompetitorGame {
    pub studio: String,
    pub studio_id: String,
    pub name: String,
    pub genre: String,
    pub theme: String,
    pub week: u32,
    /// 0..=100
    pub meta: f32,
    /// 0..=100 technical quality (for the technical award).
    pub tech: f32,
    /// Competitors' games are never indie-sized unless their studio is small.
    pub indie: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwardRecord {
    pub year: u32,
    pub award: String,
    pub winner: String,
    pub game: String,
    pub meta: f32,
    pub player: bool,
}

/// A temporary surge or slump of one genre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wave {
    pub genre: String,
    pub until_week: u32,
    pub target: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketState {
    pub genre_trend: BTreeMap<String, f32>,
    pub theme_trend: BTreeMap<String, f32>,
    pub wave: Option<Wave>,
    pub next_wave_week: u32,
    pub competitors: Vec<CompetitorState>,
    pub competitor_games: Vec<CompetitorGame>,
    pub awards: Vec<AwardRecord>,
}

const MAX_COMPETITOR_GAMES: usize = 120;
const MAX_AWARDS: usize = 60;

impl MarketState {
    pub fn genre_trend(&self, genre: &str) -> f32 {
        self.genre_trend.get(genre).copied().unwrap_or(1.0)
    }

    pub fn theme_trend(&self, theme: &str) -> f32 {
        self.theme_trend.get(theme).copied().unwrap_or(1.0)
    }

    /// Demand multiplier for a genre/theme pair right now.
    pub fn demand_trend(&self, genre: &str, theme: &str) -> f32 {
        self.genre_trend(genre) * self.theme_trend(theme).sqrt()
    }

    /// 0.78..=1.0: strong competing releases in your genre take some of the audience.
    pub fn pressure(&self, genre: &str, week: u32) -> f32 {
        let hits = self
            .competitor_games
            .iter()
            .filter(|g| g.genre == genre && g.meta >= 75.0 && week >= g.week && week - g.week <= 12)
            .count()
            .min(3);
        1.0 - 0.07 * hits as f32
    }
}

const ADJECTIVES: [&str; 20] = [
    "Neon",
    "Crimson",
    "Silent",
    "Cosmic",
    "Wild",
    "Hidden",
    "Broken",
    "Golden",
    "Iron",
    "Frozen",
    "Lucky",
    "Rogue",
    "Electric",
    "Ancient",
    "Tiny",
    "Last",
    "Endless",
    "Burning",
    "Midnight",
    "Forgotten",
];
const NOUNS: [&str; 20] = [
    "Kingdom",
    "Dungeon",
    "Racer",
    "Legacy",
    "Frontier",
    "Heist",
    "Odyssey",
    "Harvest",
    "Fortress",
    "Voyage",
    "Echo",
    "Tactics",
    "Rebellion",
    "Garden",
    "Signal",
    "Circuit",
    "Empire",
    "Blade",
    "Orbit",
    "Tales",
];

fn game_name(rng: &mut GameRng) -> String {
    let a = rng.pick(&ADJECTIVES).copied().unwrap_or("Super");
    let n = rng.pick(&NOUNS).copied().unwrap_or("Quest");
    format!("{a} {n}")
}

impl GameState {
    /// Seed trends and competitor schedules. Called lazily so old saves keep working.
    fn ensure_market(&mut self, content: &ContentLibrary) {
        if self.market.genre_trend.is_empty() {
            for g in &content.genres {
                self.market.genre_trend.insert(g.id.clone(), 1.0 + self.rng.range_f32(-0.1, 0.1));
            }
            for t in &content.themes {
                self.market.theme_trend.insert(t.id.clone(), 1.0 + self.rng.range_f32(-0.08, 0.08));
            }
            self.market.next_wave_week = self.date.week() + self.rng.range_i64(10, 30) as u32;
        }
        if self.market.competitors.is_empty() {
            for c in &content.competitors {
                let first = self.date.week() + self.rng.range_i64(6, c.interval_weeks.max(8) as i64) as u32;
                self.market.competitors.push(CompetitorState {
                    id: c.id.clone(),
                    next_release: first,
                    released: 0,
                });
            }
        }
    }

    pub(crate) fn advance_market(&mut self, content: &ContentLibrary) {
        self.ensure_market(content);
        let week = self.date.week();

        // Mean-reverting drift of every trend.
        for t in self.market.genre_trend.values_mut() {
            let noise = self.rng.noise(0.012);
            *t = (*t + (1.0 - *t) * 0.025 + noise).clamp(0.55, 1.7);
        }
        for t in self.market.theme_trend.values_mut() {
            let noise = self.rng.noise(0.008);
            *t = (*t + (1.0 - *t) * 0.02 + noise).clamp(0.7, 1.4);
        }

        // Waves: a genre booms or slumps for a while.
        if let Some(w) = self.market.wave.clone() {
            if week >= w.until_week {
                self.market.wave = None;
            } else if let Some(t) = self.market.genre_trend.get_mut(&w.genre) {
                *t += (w.target - *t) * 0.08;
            }
        } else if week >= self.market.next_wave_week && !content.genres.is_empty() {
            let i = self.rng.range_usize(0, content.genres.len() - 1);
            let genre = &content.genres[i];
            let boom = self.rng.chance(0.6);
            let target = if boom { 1.5 } else { 0.7 };
            let duration = self.rng.range_i64(12, 26) as u32;
            self.market.wave = Some(Wave { genre: genre.id.clone(), until_week: week + duration, target });
            self.market.next_wave_week = week + duration + self.rng.range_i64(12, 40) as u32;
            let text = if boom {
                format!("Market trend: {} games are booming!", genre.name)
            } else {
                format!("Market trend: {} games are falling out of fashion.", genre.name)
            };
            self.note(if boom { NoteKind::Good } else { NoteKind::Warn }, text);
        }

        self.advance_competitors(content);
        self.announce_platforms(content);
        if self.date.week_of_year() == 50 {
            self.hold_awards(content);
        }
    }

    fn advance_competitors(&mut self, content: &ContentLibrary) {
        let week = self.date.week();
        for i in 0..self.market.competitors.len() {
            if week < self.market.competitors[i].next_release {
                continue;
            }
            let id = self.market.competitors[i].id.clone();
            let Some(def) = content.competitors.iter().find(|c| c.id == id) else {
                continue;
            };
            let genre = if self.rng.chance(0.7) {
                self.rng.pick(&def.genres).cloned()
            } else {
                self.rng.pick(&content.genres).map(|g| g.id.clone())
            }
            .unwrap_or_else(|| "action".into());
            let theme = self.rng.pick(&content.themes).map(|t| t.id.clone()).unwrap_or_default();
            // Their quality grows slowly over the years as the industry matures.
            let maturity = (week as f32 / 52.0 * 1.5).min(15.0);
            let trend = self.market.genre_trend(&genre);
            let meta = (42.0 + def.strength * 40.0 + maturity + (trend - 1.0) * 8.0 + self.rng.noise(7.0))
                .clamp(25.0, 98.0);
            let tech = (meta + self.rng.noise(9.0)).clamp(20.0, 99.0);
            let name = game_name(&mut self.rng);
            let game = CompetitorGame {
                studio: def.name.clone(),
                studio_id: def.id.clone(),
                name: name.clone(),
                genre: genre.clone(),
                theme,
                week,
                meta,
                tech,
                indie: def.strength <= 0.66,
            };
            self.market.competitor_games.push(game);
            if self.market.competitor_games.len() > MAX_COMPETITOR_GAMES {
                self.market.competitor_games.remove(0);
            }
            let interval = def.interval_weeks.max(8) as f32;
            let wait = (interval * self.rng.range_f32(0.7, 1.3)).round().max(6.0) as u32;
            self.market.competitors[i].next_release = week + wait;
            self.market.competitors[i].released += 1;
            if meta >= 80.0 {
                if let Some(t) = self.market.genre_trend.get_mut(&genre) {
                    *t = (*t + 0.05).min(1.7);
                }
                let genre_name = content.genre(&genre).map(|g| g.name.clone()).unwrap_or(genre);
                self.note(
                    NoteKind::Info,
                    format!("{} released the {genre_name} hit “{name}” (Metascore {meta:.0}).", def.name),
                );
            }
        }
    }

    fn announce_platforms(&mut self, content: &ContentLibrary) {
        let week = self.date.week();
        for p in &content.platforms {
            if p.launch_week == week && p.launch_week > 0 {
                self.note(NoteKind::Good, format!("The {} has launched! New audience, new dev kit.", p.name));
            } else if p.launch_week > week && p.launch_week - week == 26 {
                self.note(NoteKind::Info, format!("Rumour: the {} launches in about six months.", p.name));
            } else if !p.is_evergreen() && p.end_week == week {
                self.note(
                    NoteKind::Warn,
                    format!("The {} has been discontinued. Its market is shrinking fast.", p.name),
                );
            }
        }
    }

    /// Yearly awards ceremony: the best games of the year, yours and your rivals'.
    fn hold_awards(&mut self, content: &ContentLibrary) {
        let year = self.date.year();
        let in_year = |w: u32| w / 52 + 1 == year;
        struct Entry {
            studio: String,
            game: String,
            meta: f32,
            tech: f32,
            indie: bool,
            player: bool,
            size_index: usize,
        }
        let mut entries: Vec<Entry> = self
            .market
            .competitor_games
            .iter()
            .filter(|g| in_year(g.week))
            .map(|g| Entry {
                studio: g.studio.clone(),
                game: g.name.clone(),
                meta: g.meta,
                tech: g.tech,
                indie: g.indie,
                player: false,
                size_index: 1,
            })
            .collect();
        for g in self.games.iter().filter(|g| !g.is_dlc && in_year(g.release_week)) {
            entries.push(Entry {
                studio: self.studio.name.clone(),
                game: g.name.clone(),
                meta: g.metascore,
                tech: g.categories.performance,
                indie: g.size <= super::model::ProjectSize::Medium,
                player: true,
                size_index: g.size.index(),
            });
        }
        if entries.is_empty() {
            return;
        }
        let categories: [(&str, f32, i64, i64); 3] = [
            ("Game of the Year", 70.0, 25_000, 300),
            ("Best Indie Game", 65.0, 8_000, 120),
            ("Best Technical Achievement", 70.0, 6_000, 100),
        ];
        for (name, min_meta, prize, fans) in categories {
            let best = entries
                .iter()
                .filter(|e| match name {
                    "Best Indie Game" => e.indie,
                    _ => true,
                })
                .filter(|e| e.meta >= min_meta)
                .max_by(|a, b| {
                    let (x, y) = if name == "Best Technical Achievement" {
                        (a.tech, b.tech)
                    } else {
                        (a.meta, b.meta)
                    };
                    x.total_cmp(&y)
                });
            let Some(w) = best else {
                continue;
            };
            let record = AwardRecord {
                year,
                award: name.to_string(),
                winner: w.studio.clone(),
                game: w.game.clone(),
                meta: w.meta,
                player: w.player,
            };
            let (player, game, size_index, studio) =
                (w.player, w.game.clone(), w.size_index, w.studio.clone());
            self.market.awards.push(record);
            if self.market.awards.len() > MAX_AWARDS {
                self.market.awards.remove(0);
            }
            if player {
                let mult = (1 + size_index) as i64;
                self.studio.money += prize * mult.min(3);
                self.studio.reputation += fans * mult;
                self.progress.bump("awards_won", 1);
                self.note(
                    NoteKind::Good,
                    format!(
                        "🏆 “{game}” wins {name} at the Golden Ferris Awards! Prize: {}.",
                        crate::fmt::money(prize * mult.min(3))
                    ),
                );
            } else {
                self.note(NoteKind::Info, format!("Golden Ferris Awards: {studio}'s “{game}” wins {name}."));
            }
        }
        let _ = content;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Difficulty;

    fn setup() -> (ContentLibrary, GameState) {
        let lib = ContentLibrary::embedded();
        let st = GameState::new_game(&lib, "S", "F", Difficulty::Normal, Some(3));
        (lib, st)
    }

    #[test]
    fn trends_stay_in_bounds_and_competitors_release_games() {
        let (lib, mut st) = setup();
        st.studio.money = 100_000_000;
        for _ in 0..260 {
            st.advance_week_auto(&lib);
        }
        for t in st.market.genre_trend.values() {
            assert!((0.55..=1.7).contains(t));
        }
        assert!(!st.market.competitor_games.is_empty(), "rivals release games over five years");
        assert!(st.market.competitor_games.iter().all(|g| (25.0..=98.0).contains(&g.meta)));
        assert!(st.market.competitor_games.len() <= MAX_COMPETITOR_GAMES);
    }

    #[test]
    fn pressure_only_counts_recent_strong_games_in_the_same_genre() {
        let mut m = MarketState::default();
        let game = |genre: &str, week: u32, meta: f32| CompetitorGame {
            studio: "X".into(),
            studio_id: "x".into(),
            name: "G".into(),
            genre: genre.into(),
            theme: "t".into(),
            week,
            meta,
            tech: meta,
            indie: false,
        };
        assert_eq!(m.pressure("rpg", 20), 1.0);
        m.competitor_games.push(game("rpg", 15, 90.0));
        m.competitor_games.push(game("rpg", 16, 50.0));
        m.competitor_games.push(game("puzzle", 15, 95.0));
        assert!((m.pressure("rpg", 20) - 0.93).abs() < 1e-5);
        assert_eq!(m.pressure("rpg", 40), 1.0, "old releases no longer matter");
        for w in 0..10 {
            m.competitor_games.push(game("rpg", 18 + w % 2, 90.0));
        }
        assert!(m.pressure("rpg", 20) >= 0.78, "pressure is capped");
    }

    #[test]
    fn platform_launches_are_announced() {
        let (lib, mut st) = setup();
        st.studio.money = 100_000_000;
        let launch = lib.platforms.iter().filter(|p| p.launch_week > 0).map(|p| p.launch_week).min().unwrap();
        for _ in 0..launch {
            st.advance_week_auto(&lib);
        }
        assert!(
            st.feed.iter().any(|n| n.text.contains("has launched")),
            "{:?}",
            st.feed.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn awards_are_given_every_year_and_can_go_to_the_player() {
        let (lib, mut st) = setup();
        st.studio.money = 100_000_000;
        // A flawless player game released early in year 1 should beat early rivals.
        st.games.push(crate::sim::ReleasedGame {
            id: 99,
            name: "Opus".into(),
            metascore: 99.0,
            release_week: 5,
            ..Default::default()
        });
        for _ in 0..52 {
            st.advance_week_auto(&lib);
        }
        assert!(st.market.awards.iter().any(|a| a.award == "Game of the Year"));
        let goty = st.market.awards.iter().find(|a| a.award == "Game of the Year").unwrap();
        assert!(goty.player, "a 99 Metascore wins: {goty:?}");
        assert!(st.progress.stat("awards_won") >= 1);
    }
}

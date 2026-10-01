//! The news feed shown on the dashboard.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind {
    Info,
    Good,
    Warn,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub week: u32,
    pub kind: NoteKind,
    pub text: String,
}

pub const MAX_FEED: usize = 80;

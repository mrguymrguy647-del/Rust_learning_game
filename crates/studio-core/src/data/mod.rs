//! Content loading. All game content is RON data, embedded at build time (see `build.rs`)
//! and optionally overridden/extended by `.ron` files on disk — no code changes needed to
//! add challenges, codex entries, events, …
//!
//! Layout (relative to the content root):
//! * `challenges/*.ron` — each file is a list of [`Challenge`]
//! * `codex/*.ron`      — each file is a list of [`CodexEntry`]
//! * `game/*.ron`       — topics, tiers, error explanations, …

pub mod challenge;
pub mod game;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

pub use challenge::{BookLink, Challenge, ChallengeKind, Check, Quiz, Rewards};
pub use game::{AchievementDef, CodexEntry, ErrorExplainer, Identified, TierDef, TopicDef};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_content.rs"));
}

/// A content file that could not be parsed (or an inconsistency found while validating).
#[derive(Debug, Clone, PartialEq)]
pub struct ContentIssue {
    pub file: String,
    pub message: String,
}

impl fmt::Display for ContentIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

/// A raw content file (path relative to the content root + text).
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

/// Everything the game knows about, parsed and indexed.
#[derive(Debug, Clone, Default)]
pub struct ContentLibrary {
    pub topics: Vec<TopicDef>,
    pub tiers: Vec<TierDef>,
    pub challenges: Vec<Challenge>,
    pub codex: Vec<CodexEntry>,
    pub error_explainers: Vec<ErrorExplainer>,
    pub achievements: Vec<AchievementDef>,
    /// Files that failed to parse. Embedded content must have none (a unit test enforces it).
    pub issues: Vec<ContentIssue>,
    /// `id (file)` of entries that replaced an earlier entry with the same id. Expected for
    /// user overrides; for built-in content a unit test requires this to be empty.
    pub overrides: Vec<String>,
    challenge_index: HashMap<String, usize>,
}

impl ContentLibrary {
    /// Content compiled into the binary.
    pub fn embedded() -> ContentLibrary {
        ContentLibrary::from_sources(Self::embedded_sources())
    }

    /// Embedded content, then every `.ron` under `extra_dir` (later files override by id).
    pub fn load(extra_dir: Option<&Path>) -> ContentLibrary {
        let mut sources = Self::embedded_sources();
        if let Some(dir) = extra_dir {
            read_dir_sources(dir, dir, &mut sources);
        }
        ContentLibrary::from_sources(sources)
    }

    /// Content read purely from a directory (used by the validator tool).
    pub fn from_dir(dir: &Path) -> ContentLibrary {
        let mut sources = Vec::new();
        read_dir_sources(dir, dir, &mut sources);
        ContentLibrary::from_sources(sources)
    }

    fn embedded_sources() -> Vec<SourceFile> {
        embedded::EMBEDDED
            .iter()
            .map(|(path, text)| SourceFile { path: (*path).to_string(), text: (*text).to_string() })
            .collect()
    }

    pub fn from_sources(sources: Vec<SourceFile>) -> ContentLibrary {
        let mut lib = ContentLibrary::default();
        for src in &sources {
            lib.ingest(src);
        }
        lib.topics.sort_by_key(|t| t.order);
        lib.rebuild_indexes();
        lib
    }

    fn ingest(&mut self, src: &SourceFile) {
        let dir = src.path.split('/').next().unwrap_or_default();
        let file = src.path.rsplit('/').next().unwrap_or_default();
        let mut replaced = Vec::new();
        match (dir, file) {
            ("challenges", _) => replaced = merge_list(&mut self.challenges, parse(src, &mut self.issues)),
            ("codex", _) => replaced = merge_list(&mut self.codex, parse(src, &mut self.issues)),
            ("game", "topics.ron") => replaced = merge_list(&mut self.topics, parse(src, &mut self.issues)),
            ("game", "tiers.ron") => replaced = merge_list(&mut self.tiers, parse(src, &mut self.issues)),
            ("game", "compiler_errors.ron") => {
                replaced = merge_list(&mut self.error_explainers, parse(src, &mut self.issues))
            }
            ("game", "achievements.ron") => {
                replaced = merge_list(&mut self.achievements, parse(src, &mut self.issues))
            }
            _ => {}
        }
        self.overrides.extend(replaced.into_iter().map(|id| format!("{id} ({})", src.path)));
    }

    fn rebuild_indexes(&mut self) {
        self.challenge_index = self.challenges.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
    }

    pub fn challenge(&self, id: &str) -> Option<&Challenge> {
        self.challenge_index.get(id).map(|&i| &self.challenges[i])
    }

    pub fn topic(&self, id: &str) -> Option<&TopicDef> {
        self.topics.iter().find(|t| t.id == id)
    }

    pub fn tier(&self, index: usize) -> Option<&TierDef> {
        self.tiers.get(index)
    }

    pub fn explainer(&self, code: &str) -> Option<&ErrorExplainer> {
        self.error_explainers.iter().find(|e| e.id == code)
    }

    pub fn challenges_in_topic<'a>(&'a self, topic: &'a str) -> impl Iterator<Item = &'a Challenge> {
        self.challenges.iter().filter(move |c| c.topic == topic)
    }

    /// Cross-reference checks (unknown topics/prerequisites, duplicate ids, …).
    /// Returns human-readable problems; empty means the content is consistent.
    pub fn validate(&self) -> Vec<String> {
        let mut problems: Vec<String> = self.issues.iter().map(ToString::to_string).collect();
        let topic_ids: Vec<&str> = self.topics.iter().map(|t| t.id.as_str()).collect();
        let mut seen = HashMap::new();
        for c in &self.challenges {
            if seen.insert(c.id.as_str(), ()).is_some() {
                problems.push(format!("duplicate challenge id `{}`", c.id));
            }
            if !topic_ids.contains(&c.topic.as_str()) {
                problems.push(format!("{}: unknown topic `{}`", c.id, c.topic));
            }
            if !(1..=5).contains(&c.difficulty) {
                problems.push(format!("{}: difficulty must be 1..=5", c.id));
            }
            for p in &c.prerequisites {
                if self.challenge(p).is_none() {
                    problems.push(format!("{}: unknown prerequisite `{p}`", c.id));
                }
                if *p == c.id {
                    problems.push(format!("{}: is its own prerequisite", c.id));
                }
            }
            if c.is_quiz() {
                match &c.quiz {
                    None => problems.push(format!("{}: quiz kind without `quiz` payload", c.id)),
                    Some(q) => {
                        if q.options.len() < 2 {
                            problems.push(format!("{}: quiz needs at least two options", c.id));
                        }
                        if q.correct >= q.options.len() {
                            problems.push(format!("{}: quiz `correct` index out of range", c.id));
                        }
                    }
                }
            } else {
                if c.starter_code.trim().is_empty() {
                    problems.push(format!("{}: empty starter_code", c.id));
                }
                if c.hidden_tests.trim().is_empty() {
                    problems.push(format!("{}: empty hidden_tests", c.id));
                }
                if c.solution.trim().is_empty() {
                    problems.push(format!("{}: empty solution", c.id));
                }
                if c.hints.len() != 3 {
                    problems.push(format!("{}: needs exactly 3 hints (has {})", c.id, c.hints.len()));
                }
            }
        }
        problems.extend(self.find_prerequisite_cycles());
        for e in &self.codex {
            if !topic_ids.contains(&e.topic.as_str()) {
                problems.push(format!("codex {}: unknown topic `{}`", e.id, e.topic));
            }
            if let Some(ch) = &e.unlock_challenge {
                if self.challenge(ch).is_none() {
                    problems.push(format!("codex {}: unknown unlock challenge `{ch}`", e.id));
                }
            }
        }
        problems
    }

    fn find_prerequisite_cycles(&self) -> Vec<String> {
        // Iterative DFS colouring: 0 = unvisited, 1 = in progress, 2 = done.
        let mut state: HashMap<&str, u8> = HashMap::new();
        let mut problems = Vec::new();
        for start in &self.challenges {
            if state.get(start.id.as_str()).copied().unwrap_or(0) != 0 {
                continue;
            }
            let mut stack: Vec<(&Challenge, usize)> = vec![(start, 0)];
            state.insert(start.id.as_str(), 1);
            while let Some((node, next)) = stack.pop() {
                if let Some(p) = node.prerequisites.get(next) {
                    stack.push((node, next + 1));
                    if let Some(child) = self.challenge(p) {
                        match state.get(child.id.as_str()).copied().unwrap_or(0) {
                            0 => {
                                state.insert(child.id.as_str(), 1);
                                stack.push((child, 0));
                            }
                            1 => problems.push(format!(
                                "prerequisite cycle involving `{}` and `{}`",
                                node.id, child.id
                            )),
                            _ => {}
                        }
                    }
                } else {
                    state.insert(node.id.as_str(), 2);
                }
            }
        }
        problems
    }
}

fn parse<T: DeserializeOwned>(src: &SourceFile, issues: &mut Vec<ContentIssue>) -> Vec<T> {
    match ron::from_str::<Vec<T>>(&src.text) {
        Ok(list) => list,
        Err(err) => {
            issues.push(ContentIssue { file: src.path.clone(), message: err.to_string() });
            Vec::new()
        }
    }
}

/// Append `incoming`, replacing existing entries that share an id. Returns the replaced ids.
fn merge_list<T: Identified>(target: &mut Vec<T>, incoming: Vec<T>) -> Vec<String> {
    let mut replaced = Vec::new();
    for item in incoming {
        if let Some(slot) = target.iter_mut().find(|e| e.id() == item.id()) {
            replaced.push(item.id().to_string());
            *slot = item;
        } else {
            target.push(item);
        }
    }
    replaced
}

fn read_dir_sources(dir: &Path, root: &Path, out: &mut Vec<SourceFile>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_dir_sources(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            if let (Ok(text), Ok(rel)) = (fs::read_to_string(&path), path.strip_prefix(root)) {
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(SourceFile { path: rel, text });
            }
        }
    }
}

impl Identified for Challenge {
    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_replaces_by_id() {
        let mut list = vec![ErrorExplainer {
            id: "E1".into(),
            title: "a".into(),
            explanation: String::new(),
            how_to_fix: String::new(),
            book_url: String::new(),
        }];
        let mut newer = list[0].clone();
        newer.title = "b".into();
        let mut other = list[0].clone();
        other.id = "E2".into();
        let replaced = merge_list(&mut list, vec![newer, other]);
        assert_eq!(replaced, vec!["E1".to_string()]);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "b");
    }

    #[test]
    fn bad_file_becomes_issue_not_panic() {
        let lib = ContentLibrary::from_sources(vec![SourceFile {
            path: "challenges/broken.ron".into(),
            text: "[ this is not ron".into(),
        }]);
        assert_eq!(lib.issues.len(), 1);
        assert!(lib.challenges.is_empty());
    }

    #[test]
    fn embedded_content_parses_cleanly() {
        let lib = ContentLibrary::embedded();
        assert!(lib.issues.is_empty(), "content issues: {:#?}", lib.issues);
        assert!(lib.overrides.is_empty(), "duplicate ids in built-in content: {:#?}", lib.overrides);
    }
}

//! Structural checks on the built-in content (fast), plus the full compile-everything
//! verification (slow, `#[ignore]`d — also available as the `validate-challenges` binary).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashSet;
use std::time::Duration;

use studio_core::challenges::validate::{validate_all, ValidateOptions};
use studio_core::challenges::{RunnerConfig, Toolchain};
use studio_core::data::{ChallengeKind, ContentLibrary};

/// The curriculum promises at least this many verified challenges.
const MIN_CHALLENGES: usize = 80;

fn lines(text: &str) -> HashSet<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

#[test]
fn builtin_content_is_consistent() {
    let lib = ContentLibrary::embedded();
    let problems = lib.validate();
    assert!(problems.is_empty(), "content problems:\n{}", problems.join("\n"));
    assert!(lib.overrides.is_empty(), "duplicate ids: {:?}", lib.overrides);
    assert!(lib.challenges.len() >= MIN_CHALLENGES, "only {} challenges", lib.challenges.len());
    assert_eq!(lib.topics.len(), 19, "the curriculum has 19 topics");
    assert_eq!(lib.tiers.len(), 5);
}

#[test]
fn challenge_ids_are_unique_and_follow_the_naming_scheme() {
    let lib = ContentLibrary::embedded();
    let mut seen = HashSet::new();
    for c in &lib.challenges {
        assert!(seen.insert(&c.id), "duplicate challenge id {}", c.id);
        assert!(
            c.id.starts_with(&c.topic) || c.id.starts_with("perf"),
            "{} should start with its topic `{}`",
            c.id,
            c.topic
        );
        assert!(
            c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
            "{}",
            c.id
        );
    }
}

#[test]
fn every_challenge_has_complete_teaching_material() {
    let lib = ContentLibrary::embedded();
    for c in &lib.challenges {
        assert!(c.title.len() >= 3, "{}: title", c.id);
        assert!(c.story.len() >= 40, "{}: story too short", c.id);
        assert!(c.task.len() >= 20, "{}: task too short", c.id);
        assert!(c.explanation.len() >= 80, "{}: explanation (\"what you learned\") too short", c.id);
        assert!(!c.book_links.is_empty(), "{}: needs at least one Rust Book / RbE link", c.id);
        if !c.is_quiz() {
            assert!(
                !c.signatures.is_empty()
                    || c.kind == ChallengeKind::FixCompile
                    || c.kind == ChallengeKind::FindBug
                    || c.kind == ChallengeKind::Refactor
                    || c.kind == ChallengeKind::Performance,
                "{}: needs signatures",
                c.id
            );
            assert!(c.hidden_tests.contains("#[test]"), "{}: hidden tests must contain #[test]", c.id);
            for (i, h) in c.hints.iter().enumerate() {
                assert!(h.len() >= 20, "{}: hint {} is too short", c.id, i + 1);
            }
            if c.kind == ChallengeKind::Refactor {
                assert!(!c.checks.is_empty(), "{}: a Refactor challenge needs static checks", c.id);
            }
        }
    }
}

#[test]
fn every_topic_is_well_covered() {
    let lib = ContentLibrary::embedded();
    for topic in &lib.topics {
        let n = lib.challenges_in_topic(&topic.id).count();
        assert!(n >= 3, "topic `{}` has only {n} challenges", topic.id);
        let codex = lib.codex.iter().filter(|e| e.topic == topic.id).count();
        assert!(codex >= 2, "topic `{}` has only {codex} codex entries", topic.id);
    }
}

#[test]
fn all_eight_challenge_kinds_are_used_several_times() {
    let lib = ContentLibrary::embedded();
    for kind in ChallengeKind::ALL {
        let n = lib.challenges.iter().filter(|c| c.kind == kind).count();
        assert!(n >= 4, "{kind:?} is used by only {n} challenges");
    }
}

#[test]
fn codex_entries_are_complete() {
    let lib = ContentLibrary::embedded();
    let mut ids = HashSet::new();
    for e in &lib.codex {
        assert!(ids.insert(&e.id), "duplicate codex id {}", e.id);
        assert!(e.title.len() >= 5 && e.summary.len() >= 20, "{}: title/summary", e.id);
        assert!(e.body.len() >= 200, "{}: body too short", e.id);
        assert!(!e.example.trim().is_empty(), "{}: needs an example", e.id);
        assert!(!e.book_url.is_empty(), "{}: needs a reading link", e.id);
    }
}

#[test]
fn challenges_form_a_solvable_prerequisite_chain() {
    // Every prerequisite exists, lives in the same topic and appears earlier in the file order.
    let lib = ContentLibrary::embedded();
    for (i, c) in lib.challenges.iter().enumerate() {
        for p in &c.prerequisites {
            let j = lib.challenges.iter().position(|x| &x.id == p);
            assert!(j.is_some_and(|j| j < i), "{}: prerequisite `{p}` must exist and come first", c.id);
        }
    }
}

#[test]
fn inline_code_spans_are_balanced() {
    // The UI renders `backtick spans` as inline code; an odd number would flip the rest of the text.
    let lib = ContentLibrary::embedded();
    let check = |owner: &str, field: &str, text: &str| {
        assert_eq!(text.matches('`').count() % 2, 0, "{owner}: unbalanced backticks in {field}");
    };
    for c in &lib.challenges {
        check(&c.id, "story", &c.story);
        check(&c.id, "task", &c.task);
        check(&c.id, "explanation", &c.explanation);
        for h in c.hints.iter().filter(|h| !h.contains('\n')) {
            check(&c.id, "hint", h);
        }
    }
    for e in &lib.codex {
        check(&e.id, "summary", &e.summary);
        for paragraph in e.body.split("\n\n") {
            check(&e.id, "body paragraph", paragraph);
        }
    }
    for e in &lib.error_explainers {
        check(&e.id, "explanation", &e.explanation);
        check(&e.id, "how_to_fix", &e.how_to_fix);
    }
}

#[test]
fn links_point_at_real_pages() {
    let book = lines(include_str!("data/book_pages.txt"));
    let rbe = lines(include_str!("data/rbe_pages.txt"));
    let lib = ContentLibrary::embedded();
    let mut urls: Vec<(String, String)> = Vec::new();
    for c in &lib.challenges {
        urls.extend(c.book_links.iter().map(|l| (c.id.clone(), l.url.clone())));
    }
    urls.extend(
        lib.topics.iter().filter(|t| !t.book_url.is_empty()).map(|t| (t.id.clone(), t.book_url.clone())),
    );
    urls.extend(
        lib.codex.iter().filter(|e| !e.book_url.is_empty()).map(|e| (e.id.clone(), e.book_url.clone())),
    );
    urls.extend(
        lib.error_explainers
            .iter()
            .filter(|e| !e.book_url.is_empty())
            .map(|e| (e.id.clone(), e.book_url.clone())),
    );

    for (owner, url) in urls {
        let no_anchor = url.split('#').next().unwrap_or(&url);
        if let Some(page) = no_anchor.strip_prefix("https://doc.rust-lang.org/book/") {
            assert!(book.contains(page), "{owner}: unknown Rust Book page `{page}`");
        } else if let Some(page) = no_anchor.strip_prefix("https://doc.rust-lang.org/rust-by-example/") {
            assert!(rbe.contains(page), "{owner}: unknown Rust by Example page `{page}`");
        } else {
            let ok = [
                "https://doc.rust-lang.org/std/",
                "https://doc.rust-lang.org/reference/",
                "https://doc.rust-lang.org/nomicon/",
                "https://doc.rust-lang.org/cargo/",
                "https://doc.rust-lang.org/rustdoc/",
                "https://rust-lang.github.io/",
                "https://veykril.github.io/tlborm/",
                "https://nnethercote.github.io/perf-book/",
                "https://doc.rust-lang.org/error_codes/",
            ]
            .iter()
            .any(|p| no_anchor.starts_with(p));
            assert!(ok, "{owner}: unexpected link target {url}");
        }
    }
}

#[test]
fn every_error_explainer_has_text() {
    let lib = ContentLibrary::embedded();
    assert!(lib.error_explainers.len() >= 30);
    for e in &lib.error_explainers {
        assert!(e.id.starts_with('E') && e.id.len() == 5, "{}", e.id);
        assert!(e.explanation.len() > 30 && e.how_to_fix.len() > 15, "{}", e.id);
    }
}

/// Compiles every reference solution and starter with the real toolchain (minutes).
/// Run with `cargo test -p studio-core --test content -- --ignored --nocapture`.
#[test]
#[ignore = "slow: compiles every challenge twice"]
fn every_challenge_is_verified_with_the_real_compiler() {
    let lib = ContentLibrary::embedded();
    let toolchain = Toolchain::detect().expect("cargo is required");
    let root = std::env::temp_dir().join("rst_validate_test");
    let options = ValidateOptions {
        workers: std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2),
        filter: std::env::var("CHALLENGE_FILTER").ok(),
        config: RunnerConfig {
            compile_timeout: Duration::from_secs(240),
            test_timeout: Duration::from_secs(30),
            output_limit: 256 * 1024,
        },
    };
    let results = validate_all(&lib, &toolchain, &root, &options, &|v| {
        println!("{} {}", if v.ok() { "ok  " } else { "FAIL" }, v.id);
    });
    let bad: Vec<String> =
        results.iter().filter(|v| !v.ok()).map(|v| format!("{}:\n{}", v.id, v.problems.join("\n"))).collect();
    assert!(bad.is_empty(), "invalid challenges:\n{}", bad.join("\n\n"));
}

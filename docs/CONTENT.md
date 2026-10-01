# Authoring content

Everything the player sees is data. No Rust code changes are needed to add challenges, Codex
entries, events, genres, platforms, engine modules or achievements.

## Where content lives

```
content/
  challenges/NN_topic.ron   a list of Challenge values per file
  codex/*.ron               a list of CodexEntry values per file
  game/<name>.ron           topics, tiers, genres, themes, platforms, outlets, engine modules,
                            events, achievements, traits, compiler-error explanations, balance …
```

The built-in files are embedded into the binary at build time. At run time the game also reads
`<data dir>/content/` (or the folder in `RST_CONTENT_DIR`). A value whose `id` already exists
**replaces** the built-in one; a new `id` is **added**. Files are routed by path (`challenges/…`,
`codex/…`, `game/<name>.ron`). Problems (parse errors, unknown topics, broken prerequisites) are
collected and reported instead of crashing the game.

## A challenge

```ron
(
    id: "lifetimes_01_longer_name",       // starts with its topic id, lowercase + digits + _
    topic: "lifetimes",                   // see game/topics.ron
    kind: FixCompile,                     // FixCompile | Complete | FromScratch | FindBug |
                                          // PredictOutput | Refactor | Performance | CodeReview
    difficulty: 2,                        // 1..=5 (drives rewards, hint and contractor prices)
    title: "The Longer Name Wins",
    story: "…",                           // narrative framing (backtick `code` spans are rendered)
    task: "…",                            // what exactly to do
    signatures: ["pub fn pick_longer(a: &str, b: &str) -> &str"],
    starter_code: r##"…"##,               // what the editor starts with
    hidden_tests: r##"
#[test]
fn returns_the_longer_name() { assert_eq!(pick_longer("Ada", "Grace"), "Grace"); }
"##,
    checks: [ForbidWord(word: "unwrap", message: "…")],   // optional static checks
    hints: ["concept nudge", "specific direction", "partial code"],
    solution: r##"…"##,                   // the idiomatic solution shown after solving
    explanation: "…",                     // “What you learned”
    book_links: [(title: "The Rust Book: Lifetimes", url: "https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html")],
    prerequisites: [],                    // challenge ids that must be solved first
    engine_tags: ["parsing"],             // matches challenges to engine modules / projects
    test_timeout_secs: Some(40),          // optional override of the sandbox timeout
)
```

How it is run: your starter/solution goes first into `src/lib.rs`, followed by
`#[cfg(test)] mod hidden_tests { use super::*; <hidden_tests> }`, so diagnostic line numbers match
the editor. The hidden tests can use anything the player's code makes public or imports.

### Rules the validator enforces

* the **reference solution passes** all hidden tests (and static checks);
* the **starter does not** — a `FixCompile` starter must fail to compile, a `Complete`/`FindBug`
  starter must fail tests, a `Refactor` starter must pass the tests but violate a `check`, a
  `Performance` starter must be too slow;
* quiz answers are verified (see below);
* ids are unique, prerequisites exist and come earlier, every challenge has an explanation and at
  least one Rust Book / Rust by Example / std link, hints are real sentences, topics are valid.

### Kinds at a glance

| Kind | Starter | What fails the starter |
|---|---|---|
| `FixCompile` | broken code | compile error |
| `Complete` | `todo!()` bodies | tests |
| `FromScratch` | a comment | missing items (compile errors) |
| `FindBug` | compiles, wrong logic | tests |
| `Refactor` | passes the tests | static `checks` (`ForbidWord`, `ForbidText`, `RequireWord`, `RequireText`) |
| `Performance` | correct but slow | timing assertion in `hidden_tests` (use generous margins) |
| `PredictOutput` / `CodeReview` | none (multiple choice) | n/a |

### Quizzes

```ron
quiz: Some((
    question: "What does this program print?",
    code: r##"fn main() { … }"##,           // shown above the options
    options: ["…", "…", "…", "…"],
    correct: 0,
    options_are_code: false,                // true: render each option as a code block
    verify: RunOutput,                      // None | RunOutput | OnlyCorrectFailsToCompile | OnlyCorrectCompiles
    wrapper: "…{CODE}…",                    // template for the OnlyCorrect* modes
)),
```

`RunOutput` compiles and runs `code`; its stdout must equal `solution` (the text of the correct
option). `OnlyCorrectFailsToCompile` / `OnlyCorrectCompiles` put every option into `wrapper` and
check with the real compiler that exactly the correct one fails / compiles. Use `None` only for
pure reasoning questions.

### Pitfalls we hit

* `r##"…"##` ends at the first `"##`. A test containing `"####"` needs more hashes (or build the
  string with `"#".repeat(4)`).
* Sandboxed tests run with `opt-level = 1` and a CPU/memory limit: keep thread counts and input
  sizes modest, and make timing thresholds several times looser than the fix needs.
* Refactor starters must **pass** the hidden tests; put "does it borrow instead of copy?" into
  `checks`, not into pointer-equality tests.
* Anything that needs a newer compiler than the game's minimum (1.95) should be avoided.

## A Codex entry

```ron
(
    id: "codex_traits_basics",
    topic: "traits",
    title: "Traits: shared behaviour",
    summary: "One line shown under the title.",
    body: r##"Paragraphs separated by a blank line. `Inline code` is rendered as code."##,
    example: r##"trait Renderable { fn glyph(&self) -> char; }"##,
    book_url: "https://doc.rust-lang.org/book/ch10-02-traits.html",
    unlock_challenge: Some("traits_01_renderable"),   // None = unlocked from the start
)
```

Reading links must point at a real page: the content test checks Rust Book and Rust by Example
slugs against `crates/studio-core/tests/data/*.txt` and allows `std`, the Reference, the Nomicon,
Cargo/rustdoc books and a few well-known sites.

## Verifying your content

```sh
cargo test -p studio-core --test content                                  # fast structural checks
cargo run -p studio-core --bin validate-challenges -- --filter my_topic_  # real-compiler check
```

Other data files are validated at load time (unknown genre/platform/module ids, cyclic engine
modules, missing outlets, …); run the app tests to see every problem at once.

# Rust Studio Tycoon — Design Document & Milestone Plan

> Living document. The milestone checklist at the bottom is kept up to date as work lands.

## 1. Pitch

You are an indie developer in a bedroom. You build your own engine **in Rust**, ship games with it,
grow into a studio, and eventually run an AAA headquarters. Progress is gated by *real* Rust
problems: the in-game editor compiles your code with the real `cargo`, runs hidden unit tests and
reports compiler errors in plain language. The tycoon loop is the motivation; the Rust curriculum
is the point.

## 2. Architecture

Cargo workspace, two crates, strict separation of logic and UI:

```
crates/studio-core   (lib, no UI deps — everything here is unit-tested)
  data/        content schema + loader (embedded RON, optional on-disk overrides)
  challenges/  sandbox project, cargo runner, diagnostics parser, test-output parser,
               static checks, grading, challenge validator
  sim/         economy, staff, engine research, projects, quality & reviews, sales,
               market, events, post-launch, player progress, autoplay bot (balance tests)
  save/        versioned JSON save slots + autosave + settings
  bin/validate-challenges   compiles every reference solution + starter against its tests
crates/studio-app    (bin `rust-studio-tycoon`: eframe/egui UI)
  ui/          one module per screen; `App::ui(&egui::Context)` is headless-testable
content/       ALL game data as RON — challenges, codex, genres, platforms, engine modules,
               events, achievements, compiler-error explanations. No code change to add content.
```

Key decisions

| Topic | Decision | Why |
|---|---|---|
| Content format | **RON**, raw strings (`r#"…"#`) for code | Code-friendly, serde-native, good error positions |
| Content delivery | `build.rs` embeds `content/**` via `include_str!` (tracked by cargo) + optional runtime override dir (`<data>/content`, or `RST_CONTENT_DIR`) | Single-binary distribution *and* mod-ability without recompiling |
| Challenge execution | Persistent sandbox cargo project + shared `target/`; **two phases**: `cargo test --no-run --message-format=json` (compile timeout) then run the test binary directly (run timeout, output cap, rlimits, process-group kill) | Fast incremental builds, separate "compile error" vs "test failure" vs "timeout", reliable kill of runaway code |
| Test injection | `lib.rs` = *player code first* + `#[cfg(test)] mod hidden_tests { use super::*; … }` | Line numbers in diagnostics match the editor 1:1 |
| Async/threads | std-only (hand-rolled `block_on` for the async topic) | Zero external deps ⇒ offline-safe, fast |
| RNG | own PCG struct implementing serde (state is saved) | Deterministic, save/load-stable simulations |
| Time | 1 tick = 1 week, 52 weeks/year; sim never advances while a challenge is pending | Tycoon pacing; challenge pauses the clock |
| UI | `eframe` (glow) + `egui_extras` syntax highlighting; background thread + channel for compiles | Never blocks UI |
| Lints | `unwrap_used`/`expect_used` = warn (denied in CI), allowed in tests | "No unwrap in production paths" |

## 3. Challenge system

Each challenge is data (`content/challenges/*.ron`, a list per file):
`id, topic, kind, difficulty(1-5), title, story, task, signatures, starter_code, hidden_tests,
checks, hints[3], solution, explanation, book_links, rewards, prerequisites, engine_tags, quiz?`

Kinds: `FixCompile`, `Complete`, `FromScratch`, `FindBug`, `PredictOutput` (quiz), `Refactor`
(tests + static checks e.g. "no `.clone()`", "no `unwrap`", "no `for` loops"), `Performance`
(large inputs, timing assertions + global timeout), `CodeReview` (quiz: which snippet is buggy/unsafe).

Verdict pipeline: write project → compile → (compile error? → parsed diagnostics + friendly
explanation from `content/game/compiler_errors.ron`) → run tests → parse libtest output → static
checks → `Verdict::{Passed, CompileError, TestsFailed, Timeout, CheckFailed, …}`.

Validator (`validate-challenges`, also an `#[ignore]`d test): for every code challenge the
**solution must be a full pass** and the **starter must NOT be a full pass**; for quizzes the
answer index must be in range. A fast always-on structural test checks ids, prerequisites,
hints, topic coverage, ≥80 challenges, kind variety.

Failure handling: unlimited retries; tiered hints (concept / direction / partial code) cost money
or time (with senior-staff freebies); after N failures a **contractor** can be hired (expensive,
solution + explanation still shown). The sandbox never permanently blocks progress.

Sandbox safety: temp/sandbox dir only, env cleared (PATH/CARGO_HOME/RUSTUP_HOME kept), strict
timeouts, 256 KiB output cap per stream, `setrlimit` (CPU/AS/FSIZE) for the test binary on Unix,
whole-process-group kill. The README states plainly that player code runs locally.

## 4. Tycoon simulation

* **Studio tiers**: Bedroom → Garage → Small Office → Studio → AAA HQ (staff slots, max project
  size, rent, max challenge difficulty, loan limit, engine licensing slots).
* **Engine**: ~12 modules (core loop, 2D renderer, physics, audio, scripting, networking, 3D, AI,
  editor tools, job system, math lib, asset pipeline). A module needs *specific challenges solved*
  + research points + money; it grants quality bonuses and unlocks genres/sizes/platforms.
* **Projects**: genre × topic × platform × audience × size, 5 focus sliders. Phases: Concept →
  Core → Content → Polish. Challenges fire at phase boundaries (pool filtered by unlocked
  topics, tier difficulty, and engine tags). Crunch trades morale/bugs for speed.
* **Quality & reviews**: category scores (gameplay/graphics/story/audio/performance) from
  focus × staff skill × engine bonuses, genre/topic fit, trend, novelty, bugs and
  *challenge performance* (first-try bonus). Five fictional outlets with different tastes and
  templated quotes.
* **Sales/economy**: launch spike × decay tail, price by size/platform, store cuts, marketing
  hype, salaries/rent weekly, loans with interest, bankruptcy warning → game over.
* **Staff**: candidates with 5 stats + traits, salaries, training, morale; seniors give free
  hints and cheaper contractors.
* **Market**: genre/topic trends (drifting cycles), fictional platform lifecycles, competitor
  studios releasing games and awards season.
* **Events** (data-driven, `events.ron`): hotfix crises (challenge), publisher deals, game jams
  (timed challenge), conferences, awards, crunch decisions.
* **Post-launch**: patches, DLC, sequels (fan carry-over), engine licensing (passive income).
* **Meta**: Rust skill level (XP), per-topic mastery, achievements (data-driven), stats,
  Rust Codex (unlocks per topic/challenge), Practice mode (no economy effect).

## 5. UX

Left nav + top status bar. Screens: Dashboard, Projects, Engine, Staff, Studio, Market,
Library (released games), Skills, Codex, Practice, Settings. Onboarding tutorial (tycoon
+ editor). Challenge screen = story/task/hints/results | syntax-highlighted editor with line
numbers, Tab indent, auto-indent. 4 save slots + autosave, versioned.
Settings: difficulty (economy only), hint-cost multiplier, sandbox timeout, editor font size.

## 6. Milestones

1. **Skeleton** — workspace, UI shell (nav, status bar, screens stubs), settings, save/load, tests.
2. **Challenge system E2E** — editor, sandbox runner, diagnostics, hints, contractor, validator, first 10 challenges.
3. **Core tycoon loop** — projects, phases, release, sales, reviews, money, new-game flow.
4. **Engine + staff + studio upgrades.**
5. **Market, events, reviews polish, post-launch.**
6. **Full curriculum (80+ verified challenges) + Rust Codex.**
7. **Tutorial, balancing (autoplay tests), polish, README, CI.**

## 7. Status

- [x] M1 Skeleton (workspace, content loader, settings, versioned saves, UI shell, headless UI tests)
- [x] M2 Challenge system (real-cargo runner, diagnostics + friendly errors, hints/contractor, editor UI, validator, first 10 challenges)
- [x] M3 Core tycoon loop (projects, phases, blockers, quality, reviews, sales, bankruptcy, Projects/Library/Dashboard UI)
- [x] M4 Engine, staff, studio (21-module research tree, hiring/training/morale/seniors, tier upgrades, loans)
- [x] M5 Market, events, post-launch (trends, platform lifecycles, rivals, awards, 27 data-driven events incl. hotfix/jam challenges, contracts, marketing, patches/DLC/sequels, licensing)
- [x] M6 Full curriculum + Codex (92 challenges across all 19 topics and all 8 kinds, every one verified against the real compiler; 57 Codex entries + Codex screen; quiz answers verified by running or compiling the snippets)
- [x] M7 Tutorial (tycoon tour + editor tour, replayable from Settings), balance (autoplay bot + regression tests, retuned economy), README/CONTENT docs, CI

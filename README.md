# Rust Studio Tycoon

A game-development tycoon, written in Rust, that **teaches Rust**. You start alone in a bedroom,
build your own engine, ship games and grow a studio — but development keeps getting blocked by
real code problems that you solve by writing real Rust, compiled by your own `cargo`.

> Work in progress — see [docs/DESIGN.md](docs/DESIGN.md) for the design and milestone status.

## Run

```sh
cargo run --release -p rust-studio-tycoon
```

You need a Rust toolchain (<https://rustup.rs>) both to build the game and, at runtime, to check your
solutions to the coding challenges.

## Develop

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

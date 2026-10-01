//! Compiles every challenge's reference solution and starter code against its hidden tests.
//!
//! ```text
//! cargo run --release -p studio-core --bin validate-challenges -- [--filter text] [--workers N]
//!                                                                  [--content-dir DIR] [--sandbox-dir DIR]
//! ```
//! Exit status is non-zero if any challenge is invalid.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use studio_core::challenges::validate::{validate_all, ValidateOptions};
use studio_core::challenges::{RunnerConfig, Toolchain};
use studio_core::data::ContentLibrary;

fn main() -> ExitCode {
    let mut filter = None;
    let mut workers = std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2);
    let mut content_dir: Option<PathBuf> = None;
    let mut sandbox_dir = std::env::temp_dir().join("rst_validate");

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--filter" => filter = args.next(),
            "--workers" => workers = args.next().and_then(|v| v.parse().ok()).unwrap_or(workers),
            "--content-dir" => content_dir = args.next().map(PathBuf::from),
            "--sandbox-dir" => {
                if let Some(d) = args.next() {
                    sandbox_dir = PathBuf::from(d);
                }
            }
            "-h" | "--help" => {
                println!("usage: validate-challenges [--filter TEXT] [--workers N] [--content-dir DIR] [--sandbox-dir DIR]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument `{other}`");
                return ExitCode::from(2);
            }
        }
    }

    let toolchain = match Toolchain::detect() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}\n\n{}", studio_core::challenges::INSTALL_INSTRUCTIONS);
            return ExitCode::from(2);
        }
    };
    println!("{} / {}", toolchain.cargo_version, toolchain.rustc_version);

    let content = match &content_dir {
        Some(dir) => ContentLibrary::from_dir(dir),
        None => ContentLibrary::embedded(),
    };
    let mut problems = content.validate();
    println!(
        "{} challenges, {} topics, {} codex entries",
        content.challenges.len(),
        content.topics.len(),
        content.codex.len()
    );

    let options = ValidateOptions {
        workers,
        filter,
        config: RunnerConfig {
            compile_timeout: Duration::from_secs(240),
            test_timeout: Duration::from_secs(30),
            output_limit: 256 * 1024,
        },
    };
    let results = validate_all(&content, &toolchain, &sandbox_dir, &options, &|v| {
        let mark = if v.ok() { "ok  " } else { "FAIL" };
        println!(
            "{mark} {:<44} {:<14} solution: {:<22} starter: {:<28} {:>5} ms",
            v.id,
            format!("{:?}", v.kind),
            v.solution_result,
            v.starter_result,
            v.millis
        );
    });

    let mut failed = 0;
    for v in results.iter().filter(|v| !v.ok()) {
        failed += 1;
        println!("\n--- {} ---", v.id);
        for p in &v.problems {
            println!("{p}");
        }
    }
    for p in problems.drain(..) {
        println!("content problem: {p}");
        failed += 1;
    }
    println!("\n{} validated, {} problem(s)", results.len(), failed);
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

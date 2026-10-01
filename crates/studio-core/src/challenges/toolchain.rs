//! Detecting the player's Rust toolchain.

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    /// Command used to launch cargo (a full path or just `cargo`).
    pub cargo: PathBuf,
    pub cargo_version: String,
    pub rustc_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainError {
    /// No `cargo` could be started at all.
    NotFound,
    /// `cargo` exists but `cargo --version` / `rustc --version` failed.
    Broken(String),
}

impl std::fmt::Display for ToolchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolchainError::NotFound => write!(f, "cargo was not found on this computer"),
            ToolchainError::Broken(why) => write!(f, "cargo is installed but not working: {why}"),
        }
    }
}

impl std::error::Error for ToolchainError {}

/// Shown when no toolchain is detected.
pub const INSTALL_INSTRUCTIONS: &str = "\
Rust Studio Tycoon checks your code with the real Rust toolchain, so you need to install it once:

  • Linux / macOS:   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  • Windows:         download and run rustup-init.exe from https://rustup.rs
                     (it also asks you to install the Visual Studio C++ build tools)

Then restart your terminal (so `cargo` is on your PATH) and press “Detect again”.
Until then you can still run your studio — you just cannot compile challenge code
(a contractor can solve challenges for you, at a price).";

fn candidates() -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "cargo.exe" } else { "cargo" };
    let mut list = vec![PathBuf::from("cargo")];
    if let Some(home) = env::var_os("CARGO_HOME") {
        list.push(PathBuf::from(home).join("bin").join(exe));
    }
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        list.push(PathBuf::from(home).join(".cargo").join("bin").join(exe));
    }
    list
}

fn version_of(program: &PathBuf, arg: &str) -> Result<String, String> {
    let out = Command::new(program).arg(arg).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Toolchain {
    pub fn detect() -> Result<Toolchain, ToolchainError> {
        let mut broken: Option<String> = None;
        for cargo in candidates() {
            match version_of(&cargo, "--version") {
                Ok(cargo_version) => {
                    // rustc lives next to cargo when installed through rustup; otherwise rely on PATH.
                    let rustc_next_to_cargo = cargo
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .map(|p| p.join(if cfg!(windows) { "rustc.exe" } else { "rustc" }));
                    let rustc_version = rustc_next_to_cargo
                        .and_then(|p| version_of(&p, "--version").ok())
                        .or_else(|| version_of(&PathBuf::from("rustc"), "--version").ok())
                        .unwrap_or_else(|| "rustc (version unknown)".to_string());
                    return Ok(Toolchain { cargo, cargo_version, rustc_version });
                }
                Err(e) => {
                    // Only a failure that is not "program missing" counts as broken.
                    if !e.contains("No such file") && !e.contains("not found") && !e.contains("cannot find") {
                        broken.get_or_insert(e);
                    }
                }
            }
        }
        match broken {
            Some(why) => Err(ToolchainError::Broken(why)),
            None => Err(ToolchainError::NotFound),
        }
    }

    /// A directory to add to PATH so cargo can find its sibling `rustc`.
    pub fn bin_dir(&self) -> Option<PathBuf> {
        self.cargo.parent().filter(|p| !p.as_os_str().is_empty()).map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_toolchain_running_these_tests() {
        let tc = Toolchain::detect().expect("tests are run with cargo");
        assert!(tc.cargo_version.starts_with("cargo "), "{}", tc.cargo_version);
        assert!(tc.rustc_version.starts_with("rustc "), "{}", tc.rustc_version);
    }

    #[test]
    fn instructions_mention_rustup() {
        assert!(INSTALL_INSTRUCTIONS.contains("rustup"));
    }
}

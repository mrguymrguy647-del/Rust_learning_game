//! The isolated cargo project player code is compiled in.
//!
//! One persistent project per worker keeps `target/` warm, so after the first (slow) build each
//! check only recompiles a tiny crate. Player code is placed *first* in `src/lib.rs` so compiler
//! line numbers match the editor; the hidden tests are appended in a `#[cfg(test)]` module.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Sandbox {
    dir: PathBuf,
}

/// The assembled crate source plus where the player's code ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembled {
    pub source: String,
    /// Number of lines belonging to the player's code (diagnostics beyond it are in hidden tests).
    pub player_lines: u32,
}

/// Build the `lib.rs` contents: player code, then the hidden tests module.
pub fn assemble(player_code: &str, hidden_tests: &str) -> Assembled {
    let mut source = String::with_capacity(player_code.len() + hidden_tests.len() + 128);
    source.push_str(player_code);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    let player_lines = source.lines().count() as u32;
    source.push_str("\n#[cfg(test)]\n#[allow(unused_imports)]\nmod hidden_tests {\n    use super::*;\n");
    source.push_str(hidden_tests);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push_str("}\n");
    Assembled { source, player_lines }
}

impl Sandbox {
    /// `<root>/w<worker>`: separate workers never share a target dir, so they can build in parallel.
    pub fn new(root: &Path, worker: usize) -> Sandbox {
        Sandbox { dir: root.join(format!("w{worker}")) }
    }

    pub fn project_dir(&self) -> PathBuf {
        self.dir.join("project")
    }

    pub fn target_dir(&self) -> PathBuf {
        self.dir.join("target")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.project_dir().join("Cargo.toml")
    }

    fn lib_path(&self) -> PathBuf {
        self.project_dir().join("src").join("lib.rs")
    }

    /// Create (or refresh) the project skeleton. `dependencies` are `name = "version"` lines
    /// that get pre-built once so later checks stay fast; the game itself needs none.
    pub fn prepare(&self, dependencies: &[String]) -> io::Result<()> {
        fs::create_dir_all(self.project_dir().join("src"))?;
        let mut manifest = String::from(
            "[package]\nname = \"player_code\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [lib]\npath = \"src/lib.rs\"\n\n[dependencies]\n",
        );
        for dep in dependencies {
            manifest.push_str(dep);
            manifest.push('\n');
        }
        manifest.push_str(
            "\n[profile.dev]\nopt-level = 1\ndebug = 0\nincremental = true\n\n\
             [lints.rust]\ndead_code = \"allow\"\n",
        );
        // Only rewrite when changed so cargo's fingerprints stay valid.
        let path = self.manifest_path();
        if fs::read_to_string(&path).map(|old| old != manifest).unwrap_or(true) {
            fs::write(&path, manifest)?;
        }
        if !self.lib_path().exists() {
            fs::write(self.lib_path(), "")?;
        }
        Ok(())
    }

    pub fn write_source(&self, source: &str) -> io::Result<()> {
        fs::write(self.lib_path(), source)
    }

    pub fn clean(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_code_comes_first_and_lines_are_counted() {
        let a = assemble("fn a() {}\nfn b() {}", "#[test]\nfn t() {}");
        assert_eq!(a.player_lines, 2);
        assert!(a.source.starts_with("fn a() {}\nfn b() {}\n"));
        assert!(a.source.contains("mod hidden_tests {\n    use super::*;\n#[test]"));
        assert!(a.source.trim_end().ends_with('}'));
    }

    #[test]
    fn prepare_is_idempotent_and_writes_manifest() {
        let root = std::env::temp_dir().join(format!("rst_sbx_{}", std::process::id()));
        let sb = Sandbox::new(&root, 0);
        sb.prepare(&[]).unwrap();
        let first = fs::metadata(sb.manifest_path()).unwrap().modified().unwrap();
        sb.prepare(&[]).unwrap();
        let second = fs::metadata(sb.manifest_path()).unwrap().modified().unwrap();
        assert_eq!(first, second, "manifest must not be rewritten when unchanged");
        let text = fs::read_to_string(sb.manifest_path()).unwrap();
        assert!(text.contains("name = \"player_code\""));
        sb.write_source("pub fn x() {}").unwrap();
        sb.clean().unwrap();
        let _ = fs::remove_dir_all(&root);
    }
}

//! Where the game keeps its files (saves, settings, sandbox, optional content overrides).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// `RST_DATA_DIR` if set, otherwise the platform data directory, otherwise `./rst_data`.
    pub fn default_location() -> AppPaths {
        if let Some(dir) = std::env::var_os("RST_DATA_DIR") {
            return AppPaths::at(PathBuf::from(dir));
        }
        let root = ProjectDirs::from("dev", "RustStudio", "RustStudioTycoon")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("rst_data"));
        AppPaths::at(root)
    }

    pub fn at(root: impl Into<PathBuf>) -> AppPaths {
        AppPaths { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn saves_dir(&self) -> PathBuf {
        self.root.join("saves")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// Persistent cargo project(s) used to compile player code.
    pub fn sandbox_dir(&self) -> PathBuf {
        self.root.join("sandbox")
    }

    /// Extra `.ron` content (challenges, codex, …) that overrides/extends the built-in set.
    pub fn content_dir(&self) -> PathBuf {
        match std::env::var_os("RST_CONTENT_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => self.root.join("content"),
        }
    }
}

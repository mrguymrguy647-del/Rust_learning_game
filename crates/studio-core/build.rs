//! Embeds every file under `<workspace>/content` into the binary via `include_str!`.
//!
//! `include_str!` is tracked by cargo, and `rerun-if-changed` on the directory makes
//! newly added content files get picked up, so editing or adding a `.ron` file always
//! triggers a rebuild — no code changes needed to ship new challenges.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((rel, path.clone()));
            }
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let content = manifest.join("../../content");
    let content = content.canonicalize().unwrap_or(content);
    println!("cargo:rerun-if-changed={}", content.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut files = Vec::new();
    collect(&content, &content, &mut files);

    let mut code = String::from("/// (relative path, file contents) of every embedded content file.\n");
    code.push_str("pub static EMBEDDED: &[(&str, &str)] = &[\n");
    for (rel, abs) in &files {
        let _ = writeln!(code, "    ({:?}, include_str!({:?})),", rel, abs.display().to_string());
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_default()).join("embedded_content.rs");
    if let Err(err) = fs::write(&out, code) {
        panic!("cannot write {}: {err}", out.display());
    }
}

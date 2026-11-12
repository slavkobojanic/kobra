//! The compatibility suite (`docs/design/48-releases-and-versioning.md`,
//! "Lockfile format versions" and "Wire and stored formats across a
//! rolling deploy"; `docs/compiler/release.md` §4 and §12 steps 5 and 10):
//! fixtures recorded under `tests/compat/` that every later toolchain must
//! keep reading.
//!
//! * [`locks`]: the golden lockfiles of every format, under
//!   `tests/compat/locks/<file>/<format>/`.
//! * [`wire`]: requests, responses and stored rows recorded from the
//!   examples, under `tests/compat/wire/<minor>/`.
//!
//! ```text
//! cargo test -p k_server --test it -- compat::
//! K_BLESS=1 cargo test -p k_server --test it -- compat::wire::   # re-record this minor
//! ```

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

mod locks;
mod wire;

use std::path::{Path, PathBuf};

/// The repository's `tests/compat/`.
fn compat_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/compat")
}

/// The files below `dir` with their paths relative to `root` (`/`
/// separated), skipping hidden directories and build output.
fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "dist" {
                collect(&path, root, out);
            }
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((relative, text));
    }
}

//! The `dist/` layout written by `k build` (`docs/compiler/codegen.md`,
//! "Output layout"):
//!
//! ```text
//! dist/
//!   app                     the server executable (k_server packaging); absent for static-only apps
//!   public/
//!     index.html            only for static-only apps
//!     assets/*.js *.css *.map   hashed client files
//!   manifest.json           the build manifest
//! ```

use std::fs;
use std::io;
use std::path::{Component, Path};

use crate::Output;

/// Writes `output.client` below `dist/public/assets/` and the manifest to
/// `dist/manifest.json`. Existing files are replaced, and every other file
/// below `dist/public/assets/` (the hashed chunks, styles and maps of
/// earlier builds) is removed, with the directories that empties; nothing
/// else in `dist`, and nothing outside it, is touched. Packaging `dist/app`
/// is the server runtime's job.
///
/// # Errors
///
/// I/O failures, and [`io::ErrorKind::InvalidInput`] for an output path
/// that would leave the directory (`..`, absolute paths).
pub fn write_public(output: &Output, dist: &Path) -> io::Result<()> {
    let assets = dist.join("public").join("assets");
    fs::create_dir_all(&assets)?;
    let wanted: std::collections::BTreeSet<std::path::PathBuf> =
        output.client.iter().map(|f| assets.join(&f.path)).collect();
    remove_stale(&assets, &wanted)?;
    for file in &output.client {
        let relative = Path::new(&file.path);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("bad output path {}", file.path),
            ));
        }
        let target = assets.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, &file.contents)?;
    }
    let manifest = serde_json::to_string_pretty(&output.manifest).map_err(io::Error::other)?;
    fs::write(dist.join("manifest.json"), manifest + "\n")
}

/// Removes every file below `dir` that is not in `wanted`, then every
/// directory below `dir` left empty. Symbolic links are removed as files,
/// never followed. Returns whether `dir` itself ended up empty.
fn remove_stale(
    dir: &Path,
    wanted: &std::collections::BTreeSet<std::path::PathBuf>,
) -> io::Result<bool> {
    let mut empty = true;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if remove_stale(&path, wanted)? {
                fs::remove_dir(&path)?;
            } else {
                empty = false;
            }
        } else if wanted.contains(&path) {
            empty = false;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileKind, OutputFile};

    /// A fresh directory for one test.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("k_dist_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    fn output(paths: &[&str]) -> Output {
        Output {
            client: paths
                .iter()
                .map(|p| OutputFile {
                    path: (*p).to_owned(),
                    contents: format!("// {p}"),
                    kind: FileKind::JavaScript,
                })
                .collect(),
            ..Output::default()
        }
    }

    #[test]
    fn a_second_write_removes_what_the_first_wrote_and_the_second_did_not() {
        let dist = scratch("stale");
        write_public(&output(&["a.11111111.js", "pages/p.22222222.js"]), &dist).unwrap_or(());
        write_public(&output(&["a.33333333.js"]), &dist).unwrap_or(());
        let assets = dist.join("public/assets");
        assert!(assets.join("a.33333333.js").is_file());
        assert!(!assets.join("a.11111111.js").exists());
        assert!(
            !assets.join("pages").exists(),
            "the emptied directory is removed"
        );
        assert!(dist.join("manifest.json").is_file());
        let _ = fs::remove_dir_all(&dist);
    }

    #[test]
    fn files_outside_the_assets_are_kept() {
        let dist = scratch("keep");
        fs::create_dir_all(dist.join("public")).unwrap_or(());
        fs::write(dist.join("public/robots.txt"), "kept").unwrap_or(());
        fs::write(dist.join("app"), "kept").unwrap_or(());
        write_public(&output(&["a.11111111.js"]), &dist).unwrap_or(());
        assert!(dist.join("public/robots.txt").is_file());
        assert!(dist.join("app").is_file());
        let _ = fs::remove_dir_all(&dist);
    }
}

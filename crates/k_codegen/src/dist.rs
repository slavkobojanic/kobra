//! The `dist/` layout written by `k build` (`docs/compiler/codegen.md`,
//! "Output layout"):
//!
//! ```text
//! dist/
//!   app                     the server executable (k_server packaging); absent for static-only apps
//!   public/
//!     index.html            only for static-only apps
//!     k-sw.js               the service worker, only for apps that notify
//!     assets/*.js *.css *.map   hashed client files
//!     assets/fonts/*.woff2      hashed fonts the themes ship
//!   manifest.json           the build manifest
//! ```

use std::fs;
use std::io;
use std::path::{Component, Path};

use crate::Output;

/// Writes `output.client` and `output.binary` (shipped fonts) below
/// `dist/public/assets/`, `output.root` (the
/// service worker) in `dist/public/`, and the manifest to
/// `dist/manifest.json`. Existing files are replaced, and every other file
/// below `dist/public/assets/` (the hashed chunks, styles and maps of
/// earlier builds) is removed, with the directories that empties; nothing
/// else in `dist`, and nothing outside it, is touched. Packaging `dist/app`
/// is the server runtime's job.
///
/// # Errors
///
/// I/O failures, and [`io::ErrorKind::InvalidInput`] for an output path
/// that would leave the directory (`..`, absolute paths). The write is not
/// atomic: on an error, stale files may already be removed and the client
/// files before the failing one written.
///
/// # Examples
///
/// ```
/// use k_codegen::{FileKind, Output, OutputFile, dist::write_public};
///
/// let dist = std::env::temp_dir().join(format!("k-codegen-doc-{}", std::process::id()));
/// let mut output = Output::default();
/// output.client.push(OutputFile {
///     path: "app.3f2a1b9c.js".into(),
///     contents: "export {};\n".into(),
///     kind: FileKind::JavaScript,
/// });
/// write_public(&output, &dist).expect("writes");
/// assert!(dist.join("public/assets/app.3f2a1b9c.js").is_file());
/// assert!(dist.join("manifest.json").is_file());
///
/// output.client[0].path = "../escape.js".into();
/// assert_eq!(write_public(&output, &dist).unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
/// # std::fs::remove_dir_all(&dist).unwrap();
/// ```
pub fn write_public(output: &Output, dist: &Path) -> io::Result<()> {
    let assets = dist.join("public").join("assets");
    fs::create_dir_all(&assets)?;
    let wanted: std::collections::BTreeSet<std::path::PathBuf> = output
        .client
        .iter()
        .map(|f| assets.join(&f.path))
        .chain(output.binary.iter().map(|f| assets.join(&f.path)))
        .collect();
    remove_stale(&assets, &wanted)?;
    let files = output
        .client
        .iter()
        .map(|f| (&f.path, f.contents.as_bytes()))
        .chain(output.binary.iter().map(|f| (&f.path, &f.bytes[..])));
    for (path, contents) in files {
        let relative = Path::new(path);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("bad output path {path}"),
            ));
        }
        let target = assets.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, contents)?;
    }
    // The files served at the site's root (`k-sw.js`): written next to
    // `assets/`, and one an earlier build wrote that this one doesn't is
    // removed (a browser keeps a registered worker whose script is gone,
    // but shows no new notifications through it, since no page asks it to).
    let public = dist.join("public");
    let worker = public.join(crate::bundle::SERVICE_WORKER);
    if !output
        .root
        .iter()
        .any(|f| f.path == crate::bundle::SERVICE_WORKER)
        && worker.is_file()
    {
        fs::remove_file(worker)?;
    }
    for file in &output.root {
        if Path::new(&file.path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || file.path.contains('/')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("bad output path {}", file.path),
            ));
        }
        fs::write(public.join(&file.path), &file.contents)?;
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
    fn the_service_worker_is_written_at_the_root_and_removed_when_gone() {
        let dist = scratch("worker");
        let mut with = output(&["a.11111111.js"]);
        with.root.push(OutputFile {
            path: "k-sw.js".into(),
            contents: "// worker".into(),
            kind: FileKind::JavaScript,
        });
        write_public(&with, &dist).unwrap_or(());
        assert!(dist.join("public/k-sw.js").is_file());
        assert!(!dist.join("public/assets/k-sw.js").exists());
        write_public(&output(&["a.11111111.js"]), &dist).unwrap_or(());
        assert!(!dist.join("public/k-sw.js").exists());
        with.root[0].path = "../k-sw.js".into();
        assert_eq!(
            write_public(&with, &dist).map_err(|e| e.kind()),
            Err(io::ErrorKind::InvalidInput)
        );
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

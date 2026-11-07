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
/// `dist/manifest.json`. Existing files are replaced; nothing outside
/// `dist` is touched. Packaging `dist/app` is the server runtime's job.
///
/// # Errors
///
/// I/O failures, and [`io::ErrorKind::InvalidInput`] for an output path
/// that would leave the directory (`..`, absolute paths).
pub fn write_public(output: &Output, dist: &Path) -> io::Result<()> {
    let assets = dist.join("public").join("assets");
    fs::create_dir_all(&assets)?;
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

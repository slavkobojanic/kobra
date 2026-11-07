//! Loading one project for the commands that work on a whole project
//! (`k routes`, `k migrate`, `k test`): find its `k.toml`, read its files
//! and analyse them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

use crate::sources;

/// A loaded project.
pub struct Project {
    /// The project directory as given on the command line.
    pub root: PathBuf,
    /// The analysis host over its files.
    pub host: AnalysisHost,
}

impl Project {
    /// Prints `error: the project has N error(s); run k check …` and returns
    /// exit status 1, for commands that need an error-free program.
    pub fn report_errors(&self, count: usize) -> ExitCode {
        eprintln!(
            "error: the project has {count} error(s); run `k check {}` to see them",
            sources::display(&self.root)
        );
        ExitCode::from(1)
    }
}

/// Reads every file of the project at `root` (the one with `k.toml`).
///
/// Prints the problem and returns exit status 2 when `root` has no
/// `k.toml`, and status 1 when some file can't be read.
pub fn load(root: &Path) -> Result<Project, ExitCode> {
    let files = read(root)?;
    Ok(Project {
        root: root.to_path_buf(),
        host: AnalysisHost::with_files(SemaOptions::new(), files),
    })
}

/// The files of the project at `root`, with the same rules as [`load`].
pub fn read(root: &Path) -> Result<Vec<(ProjectPath, String)>, ExitCode> {
    if !root.join("k.toml").is_file() {
        eprintln!(
            "error: `{}` is not a project: it has no k.toml",
            sources::display(root)
        );
        return Err(ExitCode::from(2));
    }
    let loaded = sources::load(&[root.to_path_buf()]);
    let Some(group) = loaded.groups.into_iter().next() else {
        eprintln!("error: no sources found in `{}`", sources::display(root));
        return Err(ExitCode::from(2));
    };
    if !group.problems.is_empty() {
        eprintln!("error: some files can't be read; run `k check` for details");
        return Err(ExitCode::from(1));
    }
    Ok(group.files)
}

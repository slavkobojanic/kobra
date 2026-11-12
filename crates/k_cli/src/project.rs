//! Loading one project for the commands that work on a whole project
//! (`k routes`, `k migrate`, `k test`): find its `k.toml`, read its files
//! and analyse them. A member of a workspace is analysed with the
//! libraries it depends on (`crate::workspace`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use k_sema::{AnalysisHost, ProjectPath, Workspace};

use crate::{sources, workspace};

/// A loaded project.
pub struct Project {
    /// The project directory as given on the command line.
    pub root: PathBuf,
    /// The analysis host over its files.
    pub host: AnalysisHost,
    /// The workspace root, the plan and this project's member.
    ws: (PathBuf, Workspace, usize),
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

    /// How a path of the analysis is shown: relative to the working
    /// directory, a library's file at its own place in the workspace.
    #[must_use]
    pub fn display(&self, path: &ProjectPath) -> String {
        let (root, ws, member) = &self.ws;
        let real = ws
            .real_path(*member, path)
            .unwrap_or_else(|| path.to_string());
        sources::display(&root.join(real))
    }

    /// The `[package] name` of the project.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.ws.1.members[self.ws.2].name
    }
}

/// Reads every file of the project at `root` (the one with `k.toml`), with
/// the libraries it depends on when it is a member of a workspace.
/// `command` names the command for the error about a workspace root.
///
/// Prints the problem and returns exit status 2 when `root` is not a
/// project (or is a workspace root), and status 1 when some file can't be
/// read.
///
/// # Errors
///
/// The exit status described above, after the problem was printed.
pub fn load(root: &Path, command: &str) -> Result<Project, ExitCode> {
    let (loaded, member) = workspace::load_member(root, command)?;
    Ok(project_of(root, &loaded, member))
}

/// The project of member `member` of `loaded`, shown as `root`.
#[must_use]
pub fn project_of(root: &Path, loaded: &workspace::Loaded, member: usize) -> Project {
    Project {
        root: root.to_path_buf(),
        host: loaded.host(member),
        ws: (loaded.root.clone(), loaded.workspace.clone(), member),
    }
}

/// The files of the analysis of the project at `root`, with the same rules
/// as [`load`].
///
/// # Errors
///
/// As [`load`].
// Used only by commands that need the server runtime (`k dev`, `k test --watch`).
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub fn read(root: &Path) -> Result<Vec<(ProjectPath, String)>, ExitCode> {
    let (loaded, member) = workspace::load_member(root, "test --watch")?;
    Ok(loaded.workspace.analysis_files(member))
}

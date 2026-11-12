//! Choosing the toolchain that runs a command
//! (`docs/design/48-releases-and-versioning.md`, "When the installed `k`
//! doesn't match").
//!
//! Before a command that works on a project (`k lsp` included) runs, the
//! running `k` reads the project's requirements (`[workspace] k` and every
//! `[package] k`, [`k_sema::Workspace::requirements`]) and the version its
//! `k.lock` records, then:
//!
//! 1. runs, when it satisfies every requirement and equals the lock (or
//!    there is no lock, or the lock no longer satisfies the requirements);
//! 2. otherwise executes the installed toolchain the lock names (or,
//!    without a usable lock, the newest installed release that satisfies
//!    every requirement) with the same arguments, environment and
//!    standard streams, setting `K_TOOLCHAIN_DELEGATED=1`;
//! 3. otherwise runs when it satisfies the requirements and differs from
//!    the lock only in patch (`k build` then records it);
//! 4. otherwise stops with `K8201` and exit status 2, naming the command
//!    that installs what the project needs.
//!
//! `K_TOOLCHAIN=<version>` names the toolchain for one command, project or
//! not, whatever the project asks for. A `k` started with
//! `K_TOOLCHAIN_DELEGATED` set never delegates. Installed toolchains live
//! in `$K_HOME/toolchains/<version>/k` (`K_HOME` defaults to `~/.k`).
//! Nothing is ever downloaded here.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use k_diagnostics::{Diagnostic, SourceMap, Span, TextRange, TextSize, codes, render};
use k_sema::Workspace;
use k_sema::workspace::{
    MANIFEST, Requirement, TOOLCHAIN_LOCK, Version, lock_toolchain, with_lock_toolchain,
};

use crate::cli::{Cli, Command, Format, MigrateCommand};
use crate::{sources, workspace};

/// The variable that names the toolchain for one command.
const OVERRIDE: &str = "K_TOOLCHAIN";

/// The variable a delegating `k` sets for the toolchain it runs.
const DELEGATED: &str = "K_TOOLCHAIN_DELEGATED";

/// The running toolchain's version.
#[must_use]
pub fn running() -> Version {
    // The workspace version is always a valid version.
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: None,
    })
}

/// `$K_HOME`, else `~/.k`; `None` without a home directory.
fn k_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("K_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(home));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".k"))
}

/// The executable's file name inside a toolchain directory: this
/// executable's own (`k`, `k.exe`).
fn exe_name() -> OsString {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(ToOwned::to_owned))
        .unwrap_or_else(|| OsString::from(if cfg!(windows) { "k.exe" } else { "k" }))
}

/// The executable of installed toolchain `version`, if it is installed.
fn installed_exe(version: &Version) -> Option<PathBuf> {
    let exe = k_home()?
        .join("toolchains")
        .join(version.to_string())
        .join(exe_name());
    exe.is_file().then_some(exe)
}

/// The installed toolchains, oldest first.
fn installed() -> Vec<Version> {
    let Some(dir) = k_home().map(|h| h.join("toolchains")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Version::parse(&e.file_name().to_string_lossy()))
        .filter(|v| installed_exe(v).is_some())
        .collect();
    out.sort();
    out
}

/// What a project asks of the toolchain.
struct Pins {
    /// The workspace root, as a path from the working directory.
    root: PathBuf,
    /// The manifests read: the root's `k.toml` and its members', by path
    /// relative to the root.
    manifests: Vec<(String, String)>,
    /// The valid requirements, with their manifest path and range.
    requirements: Vec<(String, TextRange, Requirement)>,
    /// `k.lock`'s text and the toolchain it records, with its line.
    lock: Option<(String, Option<(Version, TextRange)>)>,
}

/// The directory of the project `path` belongs to: its workspace root, or
/// the nearest directory at or above it with a `k.toml`.
fn project_root(path: &Path) -> Option<PathBuf> {
    if let Some(located) = workspace::locate(path) {
        return Some(located.root);
    }
    let start = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let mut dir = start.clone();
    let absolute = std::fs::canonicalize(&start).ok()?;
    for ancestor in absolute.ancestors() {
        if ancestor.join(MANIFEST).is_file() {
            return Some(dir);
        }
        dir = dir.join("..");
    }
    None
}

/// Reads the pins of the project at `path`; `None` outside a project.
fn pins(path: &Path) -> Option<Pins> {
    let root = project_root(path)?;
    let root_text = std::fs::read_to_string(root.join(MANIFEST)).ok()?;
    let mut files = vec![(MANIFEST.to_owned(), root_text.clone())];
    if Workspace::is_workspace_manifest(&root_text) {
        let (manifest, _) = k_sema::workspace::parse_manifest(MANIFEST, &root_text, true, false);
        for (dir, _) in &manifest.members {
            let dir = dir.trim_matches('/').trim_start_matches("./");
            let rel = format!("{dir}/{MANIFEST}");
            if let Ok(text) = std::fs::read_to_string(root.join(&rel)) {
                files.push((rel, text));
            }
        }
    }
    let ws = Workspace::from_files(files.clone());
    let requirements = ws
        .requirements()
        .iter()
        .map(|q| (q.path.clone(), q.range, q.requirement.clone()))
        .collect();
    let lock = std::fs::read_to_string(root.join(TOOLCHAIN_LOCK))
        .ok()
        .map(|text| {
            let parsed = lock_toolchain(&text).ok().flatten();
            (text, parsed)
        });
    Some(Pins {
        root,
        manifests: files,
        requirements,
        lock,
    })
}

/// The project directory a command works on; `None` for the commands that
/// don't work on one.
fn command_project(command: &Command) -> Option<PathBuf> {
    match command {
        Command::Check(args) => args.paths.first().cloned(),
        Command::Fix(args) => args.paths.first().cloned(),
        Command::Build(args) => Some(args.project.clone()),
        Command::Run(args) | Command::Dev(args) => Some(args.project.clone()),
        Command::Test(args) => Some(args.project.clone()),
        Command::Routes(args) => Some(args.project.clone()),
        Command::Lsp { .. } => Some(PathBuf::from(".")),
        Command::Migrate { command } => Some(match command {
            MigrateCommand::New(args) => args.project.clone(),
            MigrateCommand::Rename(args) => args.project.clone(),
            MigrateCommand::Apply(args) | MigrateCommand::Status(args) => args.project.clone(),
        }),
        Command::New(_) | Command::HashPassword | Command::Keys { .. } | Command::Debug(_) => None,
    }
}

/// The diagnostic format of a command: JSON only for `k check --format json`
/// and `k fix --format json`.
fn format_of(command: Option<&Command>) -> Format {
    match command {
        Some(Command::Check(args)) => args.format,
        Some(Command::Fix(args)) => args.format,
        _ => Format::Human,
    }
}

/// What to do with the command.
enum Choice {
    /// This toolchain runs it.
    Run,
    /// The installed toolchain at this path runs it.
    Delegate(PathBuf),
}

/// Chooses before anything is parsed: `K_TOOLCHAIN` applies to every
/// command, `--help` and `--version` included. Returns the exit status
/// when the command was delegated or stopped.
#[must_use]
pub fn before_parse() -> Option<ExitCode> {
    if std::env::var_os(DELEGATED).is_some() {
        return None;
    }
    let wanted = std::env::var(OVERRIDE).ok().filter(|v| !v.is_empty())?;
    let Some(version) = Version::parse(&wanted) else {
        eprintln!(
            "error: {OVERRIDE}=`{wanted}` is not a toolchain version; name one such as `0.1.0` or `0.2.0-rc.1`"
        );
        return Some(ExitCode::from(2));
    };
    if version == running() {
        return None;
    }
    // `k check --format json` still gets JSON.
    let command = <Cli as clap::Parser>::try_parse().ok().map(|c| c.command);
    let format = format_of(command.as_ref());
    match installed_exe(&version) {
        Some(exe) => Some(delegate(&exe)),
        None => {
            let message = format!(
                "{OVERRIDE} names k {version}, which isn't installed; this is k {}",
                running()
            );
            let help = format!("install it with `k toolchain install {version}`");
            Some(stop(None, &message, &help, format))
        }
    }
}

/// Chooses for a parsed command: delegates or stops when the project needs
/// another toolchain. Returns the exit status when it did; `None` when
/// this toolchain runs the command.
#[must_use]
pub fn before_command(cli: &Cli) -> Option<ExitCode> {
    if std::env::var_os(DELEGATED).is_some() || std::env::var_os(OVERRIDE).is_some() {
        // Delegated, or `K_TOOLCHAIN` named this toolchain.
        return None;
    }
    let path = command_project(&cli.command)?;
    let pins = pins(&path)?;
    let format = format_of(Some(&cli.command));
    if let Some((text, None)) = &pins.lock
        && let Err(e) = lock_toolchain(text)
    {
        let mut map = SourceMap::new();
        let file = map.add(
            sources::display(&pins.root.join(TOOLCHAIN_LOCK)),
            text.clone(),
        );
        return Some(emit(&e.diagnostic(file), &map, format));
    }
    match choose(&pins) {
        Ok(Choice::Run) => None,
        Ok(Choice::Delegate(exe)) => Some(delegate(&exe)),
        Err((at, message, help)) => Some(stop(Some((&pins, at)), &message, &help, format)),
    }
}

/// Where a `K8201` points: the lock's line or the first requirement's.
enum At {
    /// `k.lock`'s `toolchain` line.
    Lock(TextRange),
    /// A manifest's `k` line.
    Manifest(String, TextRange),
}

/// The decision of steps 1–4 of the module documentation.
fn choose(pins: &Pins) -> Result<Choice, (At, String, String)> {
    let me = running();
    let satisfies = |v: &Version| pins.requirements.iter().all(|(_, _, r)| r.matches(v));
    // A lock that no longer satisfies the requirements is stale.
    let lock = pins
        .lock
        .as_ref()
        .and_then(|(_, l)| l.clone())
        .filter(|(v, _)| satisfies(v));
    match &lock {
        Some((locked, _)) if *locked == me => return Ok(Choice::Run),
        None if satisfies(&me) => return Ok(Choice::Run),
        _ => {}
    }
    let target = match &lock {
        Some((locked, _)) => installed_exe(locked),
        None => installed()
            .into_iter()
            .rev()
            .find(|v| v.pre.is_none() && satisfies(v))
            .and_then(|v| installed_exe(&v)),
    };
    if let Some(exe) = target {
        return Ok(Choice::Delegate(exe));
    }
    if let Some((locked, _)) = &lock
        && satisfies(&me)
        && me.same_minor(locked)
    {
        return Ok(Choice::Run);
    }
    let needs = if pins.requirements.is_empty() {
        String::new()
    } else {
        let list: Vec<&str> = pins
            .requirements
            .iter()
            .map(|(_, _, r)| r.as_str())
            .collect();
        format!(" {}", list.join(" and "))
    };
    let (at, message, install) = match &lock {
        Some((locked, range)) => (
            At::Lock(*range),
            format!("this project needs k{needs} (k.lock: {locked}); this is k {me}"),
            format!("`k toolchain install {locked}`"),
        ),
        None => {
            let (path, range, _) = &pins.requirements[0];
            (
                At::Manifest(path.clone(), *range),
                format!("this project needs k{needs}; this is k {me}"),
                "`k toolchain install`".to_owned(),
            )
        }
    };
    let help = format!(
        "install what it needs with {install}, or move the project to this toolchain with `k upgrade`"
    );
    Err((at, message, help))
}

/// Prints `K8201` with `message` and `help` (at `at` in the project's
/// files, or at the start of an empty `K_TOOLCHAIN` source) and returns
/// exit status 2.
fn stop(at: Option<(&Pins, At)>, message: &str, help: &str, format: Format) -> ExitCode {
    let mut map = SourceMap::new();
    let span = match at {
        Some((pins, At::Lock(range))) => {
            let text = pins.lock.as_ref().map_or("", |(t, _)| t.as_str());
            let file = map.add(
                sources::display(&pins.root.join(TOOLCHAIN_LOCK)),
                text.to_owned(),
            );
            Span::new(file, range)
        }
        Some((pins, At::Manifest(path, range))) => {
            let text = pins
                .manifests
                .iter()
                .find(|(p, _)| *p == path)
                .map_or("", |(_, t)| t.as_str());
            let file = map.add(sources::display(&pins.root.join(&path)), text.to_owned());
            Span::new(file, range)
        }
        None => {
            let file = map.add(OVERRIDE.to_owned(), String::new());
            Span::empty(file, TextSize::from(0))
        }
    };
    let installed: Vec<String> = installed().iter().map(ToString::to_string).collect();
    let note = if installed.is_empty() {
        "no toolchain is installed".to_owned()
    } else {
        format!("installed toolchains: {}", installed.join(", "))
    };
    let d = Diagnostic::new(codes::TOOLCHAIN_MISMATCH, span, message)
        .with_note(note)
        .with_help(help);
    emit(&d, &map, format)
}

/// Prints `d` in `format` and returns exit status 2.
fn emit(d: &Diagnostic, map: &SourceMap, format: Format) -> ExitCode {
    match format {
        Format::Json => {
            let _ = writeln!(std::io::stdout().lock(), "{}", render::json(d, map));
        }
        Format::Human => {
            let options = if std::io::stderr().is_terminal() {
                render::HumanOptions::colored()
            } else {
                render::HumanOptions::plain()
            };
            let _ = writeln!(
                std::io::stderr().lock(),
                "{}",
                render::human(d, map, &options)
            );
        }
    }
    ExitCode::from(2)
}

/// Runs `exe` with this process's arguments, environment and standard
/// streams, plus `K_TOOLCHAIN_DELEGATED=1`, and returns its exit status.
/// On Unix the process is replaced, so signals reach the toolchain.
fn delegate(exe: &Path) -> ExitCode {
    let mut command = std::process::Command::new(exe);
    command
        .args(std::env::args_os().skip(1))
        .env(DELEGATED, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = command.exec();
        eprintln!("error: cannot run `{}`: {e}", exe.display());
        ExitCode::from(2)
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|c| u8::try_from(c).ok())
                .unwrap_or(1),
        ),
        Err(e) => {
            eprintln!("error: cannot run `{}`: {e}", exe.display());
            ExitCode::from(2)
        }
    }
}

/// Records this toolchain in the `k.lock` at `root` (a workspace root or
/// an app's directory), for `k build`: written when it names another
/// version or doesn't exist, unless this toolchain doesn't satisfy the
/// project's requirements (it runs only because `K_TOOLCHAIN` named it).
///
/// # Errors
///
/// Exit status 2, after printing the problem, when the lock can't be
/// written.
pub fn record(root: &Path) -> Result<(), ExitCode> {
    let me = running();
    if let Some(pins) = pins(root)
        && !pins.requirements.iter().all(|(_, _, r)| r.matches(&me))
    {
        return Ok(());
    }
    let path = root.join(TOOLCHAIN_LOCK);
    let old = std::fs::read_to_string(&path).ok();
    // A lock this toolchain can't read (one a newer toolchain wrote) is
    // left as it is.
    if old.as_deref().is_some_and(|t| lock_toolchain(t).is_err()) {
        return Ok(());
    }
    let new = with_lock_toolchain(old.as_deref(), &me);
    if old.as_deref() == Some(new.as_str()) {
        return Ok(());
    }
    if let Err(e) = std::fs::write(&path, new) {
        eprintln!("error: cannot write `{}`: {e}", sources::display(&path));
        return Err(ExitCode::from(2));
    }
    eprintln!("wrote {}", sources::display(&path));
    Ok(())
}

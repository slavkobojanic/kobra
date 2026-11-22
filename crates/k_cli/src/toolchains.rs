//! `k toolchain install | list | remove | default` and `k self update`
//! (`docs/design/48-releases-and-versioning.md`, "Commands";
//! `docs/compiler/release.md` §12 step 4).
//!
//! `$K_HOME` (`~/.k` by default) holds:
//!
//! | Path | What |
//! |---|---|
//! | `toolchains/<version>/` | an installed toolchain: `k` (`k.exe`), `README.md`, `LICENSE-MIT`, `LICENSE-APACHE` |
//! | `bin/k` | a copy of the default toolchain's `k`: the one on the `PATH` |
//! | `default` | the default toolchain's version, one line |
//! | `index.json` | the toolchain index as last fetched, for yanked notes offline |
//! | `tmp/` | installs in progress |
//!
//! These are the only commands that reach the network, and only the index
//! (`K_TOOLCHAIN_INDEX`, else the URL this `k` was built with) and the
//! archives it lists. An install downloads into `tmp/`, checks the size
//! and SHA-256 the index gives, unpacks there and then renames the
//! directory into `toolchains/`, so an interrupted or failed install
//! leaves no toolchain behind; what it left in `tmp/` is removed by a
//! later install once it is an hour old.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use k_sema::workspace::Version;

use crate::cli::{SelfCommand, ToolchainCommand, ToolchainInstallArgs};
use crate::index::{self, Index, Spec};
use crate::{archive, toolchain};

/// The variable naming the toolchain index's URL.
const INDEX_VAR: &str = "K_TOOLCHAIN_INDEX";

/// The index URL a release build is given (by the release pipeline, once
/// the docs domain is chosen); none until then.
const BUILT_IN_INDEX: Option<&str> = option_env!("K_DEFAULT_TOOLCHAIN_INDEX");

/// The largest index read.
const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;

/// The largest archive read.
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// How old a leftover in `tmp/` must be before an install removes it.
const STALE: Duration = Duration::from_secs(3600);

/// A failure: the message, and the exit status (1, or 2 for a usage error).
type Failure = (String, u8);

/// Runs a `k toolchain` subcommand.
#[must_use]
pub fn run(command: &ToolchainCommand) -> ExitCode {
    let result = match command {
        ToolchainCommand::Install(args) => install_command(args),
        ToolchainCommand::List => list(),
        ToolchainCommand::Remove(args) => remove(&args.toolchain),
        ToolchainCommand::Default(args) => make_default_command(&args.toolchain),
    };
    finish(result)
}

/// Runs a `k self` subcommand.
#[must_use]
pub fn run_self(command: &SelfCommand) -> ExitCode {
    match command {
        SelfCommand::Update => finish(self_update()),
    }
}

/// Prints a failure and returns its status.
fn finish(result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err((message, code)) => {
            eprintln!("error: {message}");
            ExitCode::from(code)
        }
    }
}

/// `$K_HOME`, or a failure without a home directory.
fn home() -> Result<PathBuf, Failure> {
    toolchain::k_home().ok_or_else(|| {
        (
            "no home directory: set K_HOME to where toolchains are installed".to_owned(),
            2,
        )
    })
}

/// The default toolchain's version, from `$K_HOME/default`.
pub(crate) fn default_version() -> Option<Version> {
    let text = fs::read_to_string(toolchain::k_home()?.join("default")).ok()?;
    Version::parse(text.trim())
}

/// Why `version` was yanked, as the index last fetched says; nothing is
/// fetched.
pub(crate) fn yanked(version: &Version) -> Option<String> {
    let text = fs::read_to_string(toolchain::k_home()?.join("index.json")).ok()?;
    let index = index::parse(&text, "").ok()?;
    index.release(version)?.yanked.clone()
}

/// Writes `bytes` to `path` through a temporary file and a rename.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    replace(&tmp, path)
}

/// Renames `from` onto `to`. Windows can't replace a running executable,
/// but it can rename one, so there the old file is moved aside first.
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if cfg!(windows) && to.exists() => {
            let aside = to.with_extension("old");
            let _ = fs::remove_file(&aside);
            fs::rename(to, &aside).map_err(|_| e)?;
            fs::rename(from, to)
        }
        Err(e) => Err(e),
    }
}

/// The index URL, or why there is none.
fn index_url() -> Result<String, Failure> {
    if let Some(url) = std::env::var(INDEX_VAR).ok().filter(|u| !u.is_empty()) {
        return Ok(url);
    }
    BUILT_IN_INDEX.map(ToOwned::to_owned).ok_or_else(|| {
        (
            format!(
                "this k names no toolchain index (none is published yet); \
                 set {INDEX_VAR} to the URL of an `index.json`"
            ),
            1,
        )
    })
}

/// Downloads `url` into `out`, returning its size and SHA-256.
#[cfg(feature = "server")]
fn download(
    url: &str,
    out: &mut dyn Write,
    max: u64,
) -> Result<k_server::outbound::Downloaded, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the download: {e}"))?;
    runtime.block_on(k_server::outbound::download(url, out, max))
}

/// What a download produced (without the server runtime there is none).
#[cfg(not(feature = "server"))]
struct Downloaded {
    /// The size in bytes.
    size: u64,
    /// The SHA-256.
    sha256: String,
}

/// Downloads nothing: this `k` has no HTTP client.
#[cfg(not(feature = "server"))]
fn download(url: &str, _out: &mut dyn Write, _max: u64) -> Result<Downloaded, String> {
    Err(format!(
        "cannot download `{url}`: this k was built without the server runtime (feature `server`)"
    ))
}

/// Fetches and parses the index, keeping a copy in `$K_HOME/index.json`.
fn fetch_index(home: &Path) -> Result<Index, Failure> {
    let url = index_url()?;
    let mut bytes = Vec::new();
    download(&url, &mut bytes, MAX_INDEX_BYTES).map_err(|e| (e, 1))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| (format!("the toolchain index at `{url}` is not UTF-8"), 1))?;
    let index = index::parse(&text, &url).map_err(|e| (e, 1))?;
    // The copy is a convenience (yanked notes); failing to write it isn't
    // a reason to stop.
    let _ = fs::create_dir_all(home)
        .and_then(|()| write_atomically(&home.join("index.json"), text.as_bytes()));
    Ok(index)
}

/// The target releases are chosen for.
fn target() -> Result<&'static str, Failure> {
    index::host_target().ok_or_else(|| {
        (
            "Kobra releases no toolchain for this platform; build `k` from source".to_owned(),
            1,
        )
    })
}

/// What `k toolchain install` without a version installs: the project's
/// toolchain (`k.lock`'s, else the newest its requirements allow), else
/// the newest stable release.
fn project_spec() -> Spec {
    let Some(pins) = toolchain::pins(Path::new(".")) else {
        return Spec::Stable;
    };
    let reqs: Vec<_> = pins
        .requirements
        .iter()
        .map(|(_, _, r)| r.clone())
        .collect();
    let locked = pins
        .lock
        .as_ref()
        .and_then(|(_, l)| l.as_ref().map(|(v, _)| v.clone()))
        .filter(|v| reqs.iter().all(|r| r.matches(v)));
    match locked {
        Some(v) => Spec::Exact(v),
        None if reqs.is_empty() => Spec::Stable,
        None => {
            let text = reqs
                .iter()
                .map(|r| r.as_str().to_owned())
                .collect::<Vec<_>>()
                .join(" and ");
            Spec::Matching(reqs, format!("release satisfying this project's k {text}"))
        }
    }
}

/// `k toolchain install [version]`.
fn install_command(args: &ToolchainInstallArgs) -> Result<(), Failure> {
    let spec = match &args.toolchain {
        None => project_spec(),
        Some(text) => Spec::parse(text).ok_or_else(|| {
            (
                format!(
                    "`{text}` is not a toolchain version; name one such as `0.3.1`, `0.3`, `stable` or `nightly`"
                ),
                2,
            )
        })?,
    };
    let home = home()?;
    let target = target()?;
    let index = fetch_index(&home)?;
    let version = install(&home, &index, &spec, target)?;
    if default_version().is_none() {
        eprintln!("make it the `k` on your PATH with `k toolchain default {version}`");
    }
    Ok(())
}

/// Installs what `spec` names from `index` unless it is installed, and
/// returns its version.
fn install(home: &Path, index: &Index, spec: &Spec, target: &str) -> Result<Version, Failure> {
    let (release, archive) = index.choose(spec, target).map_err(|e| (e, 1))?;
    let version = &release.version;
    if let Some(reason) = &release.yanked {
        eprintln!("warning: k {version} is yanked: {reason}");
    }
    if toolchain::installed_exe(version).is_some() {
        eprintln!("k {version} is already installed");
        return Ok(version.clone());
    }
    let tmp = home.join("tmp");
    fs::create_dir_all(&tmp).map_err(|e| (format!("cannot create `{}`: {e}", tmp.display()), 1))?;
    remove_stale(&tmp);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let staging = tmp.join(format!("install-{version}-{}-{nanos}", std::process::id()));
    let result = stage(&staging, archive, version, home);
    let _ = fs::remove_dir_all(&staging);
    result?;
    eprintln!(
        "installed k {version} ({target}) in {}",
        home.join("toolchains").join(version.to_string()).display()
    );
    Ok(version.clone())
}

/// Removes what interrupted installs left in `tmp` once it is stale.
fn remove_stale(tmp: &Path) {
    let Ok(entries) = fs::read_dir(tmp) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if old {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Downloads, checks and unpacks `archive` in `staging`, then moves the
/// toolchain into `home/toolchains/<version>`.
fn stage(
    staging: &Path,
    archive: &index::Archive,
    version: &Version,
    home: &Path,
) -> Result<(), Failure> {
    let io = |what: &str, e: std::io::Error| (format!("cannot {what}: {e}"), 1);
    fs::create_dir_all(staging).map_err(|e| io("create the install directory", e))?;
    let name = archive
        .url
        .rsplit('/')
        .next()
        .unwrap_or("archive")
        .split(['?', '#'])
        .next()
        .unwrap_or("archive")
        .to_owned();
    let file_path = staging.join(&name);
    let mut file = fs::File::create(&file_path).map_err(|e| io("create the download", e))?;
    eprintln!("downloading {}", archive.url);
    let got = download(&archive.url, &mut file, MAX_ARCHIVE_BYTES).map_err(|e| (e, 1))?;
    drop(file);
    if let Some(size) = archive.size
        && size != got.size
    {
        return Err((
            format!(
                "`{name}` is {} bytes, but the index says {size}; nothing was installed",
                got.size
            ),
            1,
        ));
    }
    if got.sha256 != archive.sha256 {
        return Err((
            format!(
                "the SHA-256 of `{name}` is {}, but the index says {}; nothing was installed",
                got.sha256, archive.sha256
            ),
            1,
        ));
    }
    let unpacked = staging.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|e| io("create the install directory", e))?;
    archive::unpack(&file_path, &name, &unpacked).map_err(|e| (e, 1))?;
    let exe = toolchain::exe_name();
    // The files are at the archive's root, or in its one directory.
    let dir = if unpacked.join(&exe).is_file() {
        unpacked
    } else {
        let mut dirs = fs::read_dir(&unpacked)
            .map_err(|e| io("read the archive", e))?
            .filter_map(Result::ok)
            .map(|e| e.path());
        match (dirs.next(), dirs.next()) {
            (Some(d), None) if d.join(&exe).is_file() => d,
            _ => {
                return Err((
                    format!(
                        "`{name}` holds no `{}`; nothing was installed",
                        exe.to_string_lossy()
                    ),
                    1,
                ));
            }
        }
    };
    let toolchains = home.join("toolchains");
    fs::create_dir_all(&toolchains).map_err(|e| io("create the toolchains directory", e))?;
    let dest = toolchains.join(version.to_string());
    if dest.exists() {
        // A broken leftover (no `k` in it, or `installed_exe` would have
        // said so): replaced.
        fs::remove_dir_all(&dest).map_err(|e| io("replace the old directory", e))?;
    }
    fs::rename(&dir, &dest).map_err(|e| io("move the toolchain into place", e))
}

/// The version of the toolchain the project in the working directory
/// uses, as delegation would choose it.
fn project_toolchain(installed: &[Version]) -> Option<Version> {
    let pins = toolchain::pins(Path::new("."))?;
    let satisfies = |v: &Version| pins.requirements.iter().all(|(_, _, r)| r.matches(v));
    if let Some(v) = pins
        .lock
        .as_ref()
        .and_then(|(_, l)| l.as_ref().map(|(v, _)| v.clone()))
        .filter(|v| satisfies(v))
    {
        return Some(v);
    }
    let me = toolchain::running();
    if satisfies(&me) {
        return Some(me);
    }
    installed
        .iter()
        .rev()
        .find(|v| v.pre.is_none() && satisfies(v))
        .cloned()
}

/// `k toolchain list`.
fn list() -> Result<(), Failure> {
    let home = home()?;
    let installed = toolchain::installed();
    let default = default_version();
    let project = project_toolchain(&installed);
    let mut out = std::io::stdout().lock();
    if installed.is_empty() {
        let _ = writeln!(
            out,
            "no toolchain is installed in {}",
            home.join("toolchains").display()
        );
    }
    for v in &installed {
        let mut tags = Vec::new();
        if default.as_ref() == Some(v) {
            tags.push("default".to_owned());
        }
        if project.as_ref() == Some(v) {
            tags.push("this project".to_owned());
        }
        if let Some(reason) = yanked(v) {
            tags.push(format!("yanked: {reason}"));
        }
        if tags.is_empty() {
            let _ = writeln!(out, "{v}");
        } else {
            let _ = writeln!(out, "{v} ({})", tags.join(", "));
        }
    }
    if let Some(p) = &project
        && !installed.contains(p)
    {
        let me = toolchain::running();
        if *p == me {
            let _ = writeln!(out, "this project uses this k, {me}");
        } else {
            let _ = writeln!(
                out,
                "this project uses k {p}, which isn't installed: `k toolchain install`"
            );
        }
    }
    Ok(())
}

/// Reads an exact version argument.
fn exact(text: &str) -> Result<Version, Failure> {
    Version::parse(text).ok_or_else(|| {
        (
            format!("`{text}` is not a toolchain version; name an installed one such as `0.3.1`"),
            2,
        )
    })
}

/// `k toolchain remove <version>`.
fn remove(text: &str) -> Result<(), Failure> {
    let version = exact(text)?;
    let home = home()?;
    let dir = home.join("toolchains").join(version.to_string());
    if !dir.is_dir() {
        return Err((format!("k {version} isn't installed"), 1));
    }
    if default_version().as_ref() == Some(&version) {
        return Err((
            format!(
                "k {version} is the default; make another toolchain the default first (`k toolchain default <version>`)"
            ),
            1,
        ));
    }
    // Moved out of `toolchains/` first, so a failure halfway leaves no
    // half-removed toolchain behind.
    let tmp = home.join("tmp");
    fs::create_dir_all(&tmp).map_err(|e| (format!("cannot remove k {version}: {e}"), 1))?;
    let aside = tmp.join(format!("remove-{version}-{}", std::process::id()));
    fs::rename(&dir, &aside).map_err(|e| (format!("cannot remove k {version}: {e}"), 1))?;
    let _ = fs::remove_dir_all(&aside);
    eprintln!("removed k {version}");
    Ok(())
}

/// `k toolchain default <version>`.
fn make_default_command(text: &str) -> Result<(), Failure> {
    let version = exact(text)?;
    make_default(&home()?, &version)
}

/// Makes installed `version` the default: copies its `k` to `bin/` and
/// records it in `default`.
fn make_default(home: &Path, version: &Version) -> Result<(), Failure> {
    let Some(exe) = toolchain::installed_exe(version) else {
        return Err((
            format!("k {version} isn't installed; install it with `k toolchain install {version}`"),
            1,
        ));
    };
    let bin = home.join("bin");
    let fail = |e: std::io::Error| (format!("cannot make k {version} the default: {e}"), 1);
    fs::create_dir_all(&bin).map_err(fail)?;
    let target = bin.join(toolchain::exe_name());
    let tmp = bin.join(format!(".k-{}.tmp", std::process::id()));
    fs::copy(&exe, &tmp).map_err(fail)?;
    replace(&tmp, &target).map_err(fail)?;
    write_atomically(&home.join("default"), format!("{version}\n").as_bytes()).map_err(fail)?;
    eprintln!("k {version} is now the default: {}", target.display());
    let on_path =
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == bin));
    if !on_path {
        eprintln!("add {} to your PATH to run it as `k`", bin.display());
    }
    Ok(())
}

/// `k self update`.
fn self_update() -> Result<(), Failure> {
    let me = toolchain::running();
    let (spec, channel) = if index::is_nightly(&me) {
        (Spec::Nightly, "nightly")
    } else {
        (Spec::Stable, "stable")
    };
    let home = home()?;
    let target = target()?;
    let index = fetch_index(&home)?;
    let version = install(&home, &index, &spec, target)?;
    if default_version().as_ref() == Some(&version)
        && home.join("bin").join(toolchain::exe_name()).is_file()
    {
        eprintln!("k {version} is the newest {channel} release and already the default");
        return Ok(());
    }
    make_default(&home, &version)
}

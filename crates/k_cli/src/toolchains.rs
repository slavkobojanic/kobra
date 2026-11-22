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
//! | `docs/<version>/` | a toolchain's book, for `k docs` |
//! | `index.json` | the toolchain index as last fetched, for yanked notes offline |
//! | `tmp/` | installs in progress |
//!
//! These are the only commands that reach the network, and only the index
//! and the archives (and books) it lists. The index is `K_TOOLCHAIN_INDEX`
//! when set, else the newest release's `index.json` in the public
//! repository `slavkobojanic/kobra`, read through GitHub's API without a
//! token, or with the user's when one is found (`crate::github`, which
//! sends it to the API only). An
//! install downloads into `tmp/`, checks the size and SHA-256 the index
//! gives, unpacks there and then renames the directory into `toolchains/`,
//! so an interrupted or failed install leaves no toolchain behind; what it
//! left in `tmp/` is removed by a later install once it is an hour old. A
//! release's book is installed the same way after its toolchain; failing
//! to install it is a warning.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use k_sema::workspace::Version;

use crate::cli::{SelfCommand, ToolchainCommand, ToolchainInstallArgs};
use crate::github::{self, Github, REPO};
use crate::index::{self, Index, Spec};
use crate::{archive, toolchain};

/// The variable naming the toolchain index's URL, instead of the
/// repository's.
const INDEX_VAR: &str = "K_TOOLCHAIN_INDEX";

/// The largest index read.
const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;

/// The largest archive read.
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// How old a leftover in `tmp/` must be before an install removes it.
const STALE: Duration = Duration::from_secs(3600);

/// A failure: the message, and the exit status (1, or 2 for a usage error).
type Failure = (String, u8);

/// Runs a `k toolchain` subcommand.
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

/// Downloads `url` into `out`, returning its size and SHA-256; `headers`
/// gives each request's headers (redirects included).
#[cfg(feature = "server")]
fn download(
    url: &str,
    out: &mut dyn Write,
    max: u64,
    headers: &dyn Fn(&str) -> Vec<(String, String)>,
) -> Result<k_server::outbound::Downloaded, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the download: {e}"))?;
    runtime.block_on(k_server::outbound::download_with(url, out, max, headers))
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
fn download(
    url: &str,
    _out: &mut dyn Write,
    _max: u64,
    _headers: &dyn Fn(&str) -> Vec<(String, String)>,
) -> Result<Downloaded, String> {
    Err(format!(
        "cannot download `{url}`: this k was built without the server runtime (feature `server`)"
    ))
}

/// Downloads a JSON document of the API (or anything else, without the
/// token) as text.
fn get_text(gh: &Github, url: &str, accept: &str, max: u64) -> Result<String, String> {
    let mut bytes = Vec::new();
    download(url, &mut bytes, max, &|u| gh.headers(u, accept)).map_err(github::explain)?;
    String::from_utf8(bytes).map_err(|_| format!("`{url}` is not UTF-8"))
}

/// Downloads a file the index lists: a release download URL of GitHub
/// (`https://github.com/<repo>/releases/download/<tag>/<file>`) as that
/// release's asset through the API (with the token when there is one),
/// anything else as it is (the token goes to the API's origin only).
fn download_file(
    gh: &Github,
    url: &str,
    out: &mut dyn Write,
    max: u64,
) -> Result<(u64, String), String> {
    if let Some((repo, tag, name)) = github::release_download(url) {
        let release_url = gh.url(&format!("repos/{repo}/releases/tags/{tag}"));
        let text = get_text(
            gh,
            &release_url,
            "application/vnd.github+json",
            MAX_INDEX_BYTES,
        )?;
        let release: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("GitHub's answer for {repo} {tag} is not JSON: {e}"))?;
        let asset = github::asset_url(&release, &name)
            .ok_or_else(|| format!("the release {tag} of {repo} has no `{name}`"))?;
        let got = download(&asset, out, max, &|u| gh.headers(u, github::OCTET_STREAM))
            .map_err(github::explain)?;
        return Ok((got.size, got.sha256));
    }
    let got = download(url, out, max, &|u| gh.json_headers(u)).map_err(github::explain)?;
    Ok((got.size, got.sha256))
}

/// Fetches and parses the index, keeping a copy in `$K_HOME/index.json`:
/// `K_TOOLCHAIN_INDEX`, else the `index.json` of the repository's newest
/// published release.
fn fetch_index(home: &Path, gh: &Github) -> Result<Index, Failure> {
    let (text, base) = match std::env::var(INDEX_VAR).ok().filter(|u| !u.is_empty()) {
        Some(url) => {
            let mut bytes = Vec::new();
            download_file(gh, &url, &mut bytes, MAX_INDEX_BYTES).map_err(|e| (e, 1))?;
            let text = String::from_utf8(bytes)
                .map_err(|_| (format!("the toolchain index at `{url}` is not UTF-8"), 1))?;
            (text, url)
        }
        None => {
            let list_url = gh.url(&format!("repos/{REPO}/releases?per_page=100"));
            let list = get_text(
                gh,
                &list_url,
                "application/vnd.github+json",
                MAX_INDEX_BYTES,
            )
            .map_err(|e| (e, 1))?;
            let list: serde_json::Value = serde_json::from_str(&list).map_err(|e| {
                (
                    format!("GitHub's list of {REPO}'s releases is not JSON: {e}"),
                    1,
                )
            })?;
            let (tag, asset) = github::newest_with(&list, "index.json").ok_or_else(|| {
                (
                    format!("no release of {REPO} has a toolchain index (`index.json`) yet"),
                    1,
                )
            })?;
            let text =
                get_text(gh, &asset, github::OCTET_STREAM, MAX_INDEX_BYTES).map_err(|e| (e, 1))?;
            (
                text,
                format!("https://github.com/{REPO}/releases/download/{tag}/index.json"),
            )
        }
    };
    let index = index::parse(&text, &base).map_err(|e| (e, 1))?;
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
    let gh = Github::from_env();
    let index = fetch_index(&home, &gh)?;
    let version = install(&home, &gh, &index, &spec, target)?;
    if default_version().is_none() {
        eprintln!("make it the `k` on your PATH with `k toolchain default {version}`");
    }
    Ok(())
}

/// Installs what `spec` names from `index` unless it is installed, and
/// returns its version.
fn install(
    home: &Path,
    gh: &Github,
    index: &Index,
    spec: &Spec,
    target: &str,
) -> Result<Version, Failure> {
    let (release, archive) = index.choose(spec, target).map_err(|e| (e, 1))?;
    let version = &release.version;
    if let Some(reason) = &release.yanked {
        eprintln!("warning: k {version} is yanked: {reason}");
    }
    if toolchain::installed_exe(version).is_some() {
        eprintln!("k {version} is already installed");
    } else {
        let staging = staging(home, &format!("install-{version}"))?;
        let result = stage(gh, &staging, archive, version, home);
        let _ = fs::remove_dir_all(&staging);
        result?;
        eprintln!(
            "installed k {version} ({target}) in {}",
            home.join("toolchains").join(version.to_string()).display()
        );
    }
    if let Some(book) = &release.docs
        && !home
            .join("docs")
            .join(version.to_string())
            .join("index.html")
            .is_file()
    {
        let result = staging(home, &format!("docs-{version}")).and_then(|dir| {
            let r = stage_docs(gh, &dir, book, version, home);
            let _ = fs::remove_dir_all(&dir);
            r
        });
        match result {
            Ok(()) => eprintln!(
                "installed the book of k {version} in {} (`k docs`)",
                home.join("docs").join(version.to_string()).display()
            ),
            Err((e, _)) => eprintln!("warning: the book of k {version} wasn't installed: {e}"),
        }
    }
    Ok(version.clone())
}

/// A new directory in `home/tmp/` for one install, named after `what`.
fn staging(home: &Path, what: &str) -> Result<PathBuf, Failure> {
    let tmp = home.join("tmp");
    fs::create_dir_all(&tmp).map_err(|e| (format!("cannot create `{}`: {e}", tmp.display()), 1))?;
    remove_stale(&tmp);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(tmp.join(format!("{what}-{}-{nanos}", std::process::id())))
}

/// The file name a URL ends in, without its query.
fn file_name(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("archive")
        .to_owned()
}

/// Downloads `archive` into `staging` and checks it against the index.
fn fetch_checked(
    gh: &Github,
    staging: &Path,
    archive: &index::Archive,
) -> Result<(PathBuf, String), Failure> {
    let io = |what: &str, e: std::io::Error| (format!("cannot {what}: {e}"), 1);
    fs::create_dir_all(staging).map_err(|e| io("create the install directory", e))?;
    let name = file_name(&archive.url);
    let file_path = staging.join(&name);
    let mut file = fs::File::create(&file_path).map_err(|e| io("create the download", e))?;
    eprintln!("downloading {}", archive.url);
    let (size, sha256) =
        download_file(gh, &archive.url, &mut file, MAX_ARCHIVE_BYTES).map_err(|e| (e, 1))?;
    drop(file);
    if let Some(expected) = archive.size
        && expected != size
    {
        return Err((
            format!(
                "`{name}` is {size} bytes, but the index says {expected}; nothing was installed"
            ),
            1,
        ));
    }
    if sha256 != archive.sha256 {
        return Err((
            format!(
                "the SHA-256 of `{name}` is {sha256}, but the index says {}; nothing was installed",
                archive.sha256
            ),
            1,
        ));
    }
    Ok((file_path, name))
}

/// Downloads, checks and unpacks a release's book in `staging`, then moves
/// it to `home/docs/<version>`.
fn stage_docs(
    gh: &Github,
    staging: &Path,
    book: &index::Archive,
    version: &Version,
    home: &Path,
) -> Result<(), Failure> {
    let io = |what: &str, e: std::io::Error| (format!("cannot {what}: {e}"), 1);
    let (file_path, name) = fetch_checked(gh, staging, book)?;
    let unpacked = staging.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|e| io("create the install directory", e))?;
    archive::unpack(&file_path, &name, &unpacked).map_err(|e| (e, 1))?;
    let dir = single_dir_with(&unpacked, "index.html")
        .ok_or_else(|| (format!("`{name}` holds no `index.html`"), 1))?;
    let docs = home.join("docs");
    fs::create_dir_all(&docs).map_err(|e| io("create the docs directory", e))?;
    let dest = docs.join(version.to_string());
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| io("replace the old book", e))?;
    }
    fs::rename(&dir, &dest).map_err(|e| io("move the book into place", e))
}

/// `dir` when it holds `file`, else its one subdirectory when that does.
fn single_dir_with(dir: &Path, file: impl AsRef<Path>) -> Option<PathBuf> {
    let file = file.as_ref();
    if dir.join(file).is_file() {
        return Some(dir.to_path_buf());
    }
    let mut dirs = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path());
    match (dirs.next(), dirs.next()) {
        (Some(d), None) if d.join(file).is_file() => Some(d),
        _ => None,
    }
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
    gh: &Github,
    staging: &Path,
    archive: &index::Archive,
    version: &Version,
    home: &Path,
) -> Result<(), Failure> {
    let io = |what: &str, e: std::io::Error| (format!("cannot {what}: {e}"), 1);
    let (file_path, name) = fetch_checked(gh, staging, archive)?;
    let unpacked = staging.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|e| io("create the install directory", e))?;
    archive::unpack(&file_path, &name, &unpacked).map_err(|e| (e, 1))?;
    let exe = toolchain::exe_name();
    // The files are at the archive's root, or in its one directory.
    let Some(dir) = single_dir_with(&unpacked, &exe) else {
        return Err((
            format!(
                "`{name}` holds no `{}`; nothing was installed",
                exe.to_string_lossy()
            ),
            1,
        ));
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
    let gh = Github::from_env();
    let index = fetch_index(&home, &gh)?;
    let version = install(&home, &gh, &index, &spec, target)?;
    if default_version().as_ref() == Some(&version)
        && home.join("bin").join(toolchain::exe_name()).is_file()
    {
        eprintln!("k {version} is the newest {channel} release and already the default");
        return Ok(());
    }
    make_default(&home, &version)
}

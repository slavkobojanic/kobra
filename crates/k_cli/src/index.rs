//! The toolchain index (`index.json`) that `k toolchain install` and `k
//! self update` read (`docs/design/48-releases-and-versioning.md`,
//! "Commands" and "The toolchain index"), and the choice of a release in it.
//!
//! ```json
//! {
//!   "format": 1,
//!   "releases": [
//!     {
//!       "version": "0.1.0",
//!       "date": "2026-12-01",
//!       "yanked": "miscompiles `match` on nested enums; use 0.1.1",
//!       "archives": [
//!         {
//!           "target": "aarch64-apple-darwin",
//!           "url": "download/v0.1.0/k-0.1.0-aarch64-apple-darwin.tar.gz",
//!           "size": 41234567,
//!           "sha256": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
//!         }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! * `format` is 1; an index of a newer format is refused with a message
//!   saying to update `k` with the install script. Unknown keys are
//!   ignored, so later formats can add fields (`signature`, when the
//!   signing scheme is wired in) without a new number.
//! * A release's channel follows from its version: no pre-release is
//!   stable, `-rc.N` a release candidate, `-nightly.<date>` a nightly.
//! * `yanked`, when present, is the reason. A yanked release is never
//!   chosen for a minor, a requirement, `stable` or `nightly`; it is
//!   installed only when named exactly.
//! * An archive's `url` is absolute, or relative to the index's own URL.
//! * `docs`, when present, is the release's book (`docs-<version>.tar.gz`,
//!   with `url`, `size` and `sha256` as an archive has), which `k toolchain
//!   install` installs for `k docs`. An index without it is still format 1.

use k_sema::workspace::{Requirement, Version};

/// The index format this toolchain reads.
pub(crate) const FORMAT: u64 = 1;

/// A parsed index.
#[derive(Debug)]
pub(crate) struct Index {
    /// Every release it lists, in the order listed.
    pub(crate) releases: Vec<Release>,
}

/// One release of the index.
#[derive(Debug)]
pub(crate) struct Release {
    /// Its version.
    pub(crate) version: Version,
    /// Why it was yanked, if it was.
    pub(crate) yanked: Option<String>,
    /// Its archives, one per target.
    pub(crate) archives: Vec<Archive>,
    /// Its book, when the index lists one (`target` is `docs`).
    pub(crate) docs: Option<Archive>,
}

/// One target's archive of a release.
#[derive(Debug)]
pub(crate) struct Archive {
    /// The target triple (`aarch64-apple-darwin`).
    pub(crate) target: String,
    /// The absolute URL.
    pub(crate) url: String,
    /// The size in bytes, when the index gives it.
    pub(crate) size: Option<u64>,
    /// The SHA-256 digest, lower-case hexadecimal.
    pub(crate) sha256: String,
}

/// What to install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Spec {
    /// Exactly this version, yanked or not.
    Exact(Version),
    /// The newest release (not a pre-release, not yanked) satisfying every
    /// requirement; the text says what was asked for, for messages.
    Matching(Vec<Requirement>, String),
    /// The newest stable release.
    Stable,
    /// The newest nightly.
    Nightly,
}

impl Spec {
    /// Reads `stable`, `nightly`, a version (`0.3.1`, `0.4.0-rc.1`,
    /// `=0.3.1`) or a requirement (`0.3`, `1`); `None` for anything else.
    pub(crate) fn parse(text: &str) -> Option<Spec> {
        match text {
            "stable" => return Some(Spec::Stable),
            "nightly" => return Some(Spec::Nightly),
            _ => {}
        }
        if let Some(v) = Version::parse(text.strip_prefix('=').unwrap_or(text)) {
            return Some(Spec::Exact(v));
        }
        Requirement::parse(text).map(|r| Spec::Matching(vec![r], format!("k {text}")))
    }
}

/// The target triple this `k` was built for, as the index names targets;
/// `None` on a platform K releases nothing for.
#[must_use]
pub(crate) fn host_target() -> Option<&'static str> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return None;
    };
    let rest = if cfg!(all(target_os = "linux", target_env = "gnu")) {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(all(target_os = "windows", target_env = "msvc")) {
        "pc-windows-msvc"
    } else {
        return None;
    };
    Some(match (arch, rest) {
        ("x86_64", "unknown-linux-gnu") => "x86_64-unknown-linux-gnu",
        ("aarch64", "unknown-linux-gnu") => "aarch64-unknown-linux-gnu",
        ("x86_64", "apple-darwin") => "x86_64-apple-darwin",
        ("aarch64", "apple-darwin") => "aarch64-apple-darwin",
        ("x86_64", _) => "x86_64-pc-windows-msvc",
        _ => "aarch64-pc-windows-msvc",
    })
}

/// `url` made absolute against `base`, the index's URL: an absolute URL is
/// kept, `/path` replaces the base's path, anything else is relative to
/// the base's directory.
#[must_use]
pub(crate) fn resolve_url(base: &str, url: &str) -> String {
    if url.contains("://") || base.is_empty() {
        return url.to_owned();
    }
    let origin_end = base
        .find("://")
        .map(|i| i + 3)
        .and_then(|start| base[start..].find('/').map(|j| start + j))
        .unwrap_or(base.len());
    if url.starts_with('/') {
        return format!("{}{url}", &base[..origin_end]);
    }
    let dir_end = base[origin_end..]
        .rfind('/')
        .map_or(base.len(), |i| origin_end + i + 1);
    let dir = &base[..dir_end];
    if dir.ends_with('/') {
        format!("{dir}{url}")
    } else {
        format!("{dir}/{url}")
    }
}

/// Parses an index fetched from `base` (archive URLs are resolved against
/// it).
///
/// # Errors
///
/// What is wrong with it: not JSON, a newer format, a release or archive
/// missing a required key or with an invalid version or digest.
pub(crate) fn parse(text: &str, base: &str) -> Result<Index, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the toolchain index is not JSON: {e}"))?;
    let format = json.get("format").and_then(serde_json::Value::as_u64);
    match format {
        Some(FORMAT) => {}
        Some(n) if n > FORMAT => {
            return Err(format!(
                "the toolchain index is format {n}, newer than this k reads ({FORMAT}); \
                 update k with the install script"
            ));
        }
        _ => return Err("the toolchain index has no valid `format`".to_owned()),
    }
    let Some(list) = json.get("releases").and_then(serde_json::Value::as_array) else {
        return Err("the toolchain index has no `releases` list".to_owned());
    };
    let mut releases = Vec::new();
    for r in list {
        let text = r
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let version = Version::parse(text)
            .ok_or_else(|| format!("the toolchain index lists an invalid version `{text}`"))?;
        let yanked = r
            .get("yanked")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let read = |a: &serde_json::Value, target: Option<&str>| -> Result<Archive, String> {
            let what = if target.is_some() {
                "the book"
            } else {
                "an archive"
            };
            let field = |key: &str| {
                a.get(key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("{what} of k {version} in the index has no `{key}`"))
            };
            let sha256 = field("sha256")?.to_ascii_lowercase();
            if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!(
                    "{what} of k {version} in the index has an invalid `sha256`"
                ));
            }
            Ok(Archive {
                target: match target {
                    Some(t) => t.to_owned(),
                    None => field("target")?.to_owned(),
                },
                url: resolve_url(base, field("url")?),
                size: a.get("size").and_then(serde_json::Value::as_u64),
                sha256,
            })
        };
        let mut archives = Vec::new();
        for a in r
            .get("archives")
            .and_then(serde_json::Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            archives.push(read(a, None)?);
        }
        let docs = match r.get("docs") {
            Some(d) if d.is_object() => Some(read(d, Some("docs"))?),
            _ => None,
        };
        releases.push(Release {
            version,
            yanked,
            archives,
            docs,
        });
    }
    Ok(Index { releases })
}

impl Release {
    /// Its archive for `target`.
    pub(crate) fn archive(&self, target: &str) -> Option<&Archive> {
        self.archives.iter().find(|a| a.target == target)
    }

    /// `true` for a nightly (`-nightly.<date>`).
    pub(crate) fn is_nightly(&self) -> bool {
        is_nightly(&self.version)
    }
}

/// `true` for a nightly version (`0.7.0-nightly.20261109`).
pub(crate) fn is_nightly(version: &Version) -> bool {
    version
        .pre
        .as_deref()
        .is_some_and(|p| p == "nightly" || p.starts_with("nightly."))
}

impl Index {
    /// The release with exactly `version`.
    pub(crate) fn release(&self, version: &Version) -> Option<&Release> {
        self.releases.iter().find(|r| r.version == *version)
    }

    /// The release `spec` names for `target`, with its archive.
    ///
    /// # Errors
    ///
    /// A message saying what the index lacks: the version, its archive for
    /// `target`, or any release that fits (naming yanked ones that would
    /// have).
    pub(crate) fn choose(&self, spec: &Spec, target: &str) -> Result<(&Release, &Archive), String> {
        if let Spec::Exact(version) = spec {
            let release = self
                .release(version)
                .ok_or_else(|| format!("k {version} is not in the toolchain index"))?;
            let archive = release
                .archive(target)
                .ok_or_else(|| format!("k {version} has no archive for {target}"))?;
            return Ok((release, archive));
        }
        let fits = |r: &Release| match spec {
            Spec::Matching(reqs, _) => {
                r.version.pre.is_none() && reqs.iter().all(|q| q.matches(&r.version))
            }
            Spec::Stable => r.version.pre.is_none(),
            Spec::Nightly => r.is_nightly(),
            Spec::Exact(_) => false,
        };
        let best = self
            .releases
            .iter()
            .filter(|r| fits(r) && r.yanked.is_none())
            .filter_map(|r| r.archive(target).map(|a| (r, a)))
            .max_by(|a, b| a.0.version.cmp(&b.0.version));
        if let Some(found) = best {
            return Ok(found);
        }
        let what = match spec {
            Spec::Matching(_, text) => text.clone(),
            Spec::Stable => "stable release".to_owned(),
            Spec::Nightly => "nightly".to_owned(),
            Spec::Exact(_) => String::new(),
        };
        let yanked: Vec<String> = self
            .releases
            .iter()
            .filter(|r| fits(r) && r.archive(target).is_some())
            .filter_map(|r| {
                r.yanked
                    .as_ref()
                    .map(|why| format!("k {} is yanked ({why})", r.version))
            })
            .collect();
        let mut message = format!("the toolchain index has no {what} for {target}");
        if !yanked.is_empty() {
            message.push_str(&format!(
                "; {}, and is installed only when named exactly",
                yanked.join(", ")
            ));
        }
        // A release candidate the spec would take once released.
        let candidate = self
            .releases
            .iter()
            .filter(|r| r.version.pre.is_some() && !r.is_nightly() && r.yanked.is_none())
            .filter(|r| match spec {
                Spec::Matching(reqs, _) => reqs.iter().all(|q| q.matches(&r.version.release())),
                Spec::Stable => true,
                Spec::Nightly | Spec::Exact(_) => false,
            })
            .filter(|r| r.archive(target).is_some())
            .max_by(|a, b| a.version.cmp(&b.version));
        if let Some(r) = candidate {
            message.push_str(&format!(
                "; k {v} is a pre-release, installed only when named exactly (`k toolchain install {v}`)",
                v = r.version
            ));
        }
        Err(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn index(releases: &[(&str, Option<&str>, &[&str])]) -> Index {
        let list: Vec<serde_json::Value> = releases
            .iter()
            .map(|(v, yanked, targets)| {
                let archives: Vec<serde_json::Value> = targets
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "target": t,
                            "url": format!("download/v{v}/k-{v}-{t}.tar.gz"),
                            "sha256": DIGEST,
                        })
                    })
                    .collect();
                let mut r = serde_json::json!({ "version": v, "archives": archives });
                if let Some(y) = yanked {
                    r["yanked"] = serde_json::json!(y);
                }
                r
            })
            .collect();
        let text = serde_json::json!({ "format": 1, "releases": list }).to_string();
        match parse(&text, "http://host.example/k/index.json") {
            Ok(i) => i,
            Err(e) => panic!("{e}"),
        }
    }

    fn chosen(index: &Index, spec: &str) -> Result<String, String> {
        let spec = Spec::parse(spec).ok_or("no spec")?;
        index.choose(&spec, "t").map(|(r, _)| r.version.to_string())
    }

    #[test]
    fn urls_resolve_against_the_index() {
        let base = "http://host.example/k/index.json";
        assert_eq!(
            resolve_url(base, "a/b.tar.gz"),
            "http://host.example/k/a/b.tar.gz"
        );
        assert_eq!(
            resolve_url(base, "/a.tar.gz"),
            "http://host.example/a.tar.gz"
        );
        assert_eq!(
            resolve_url(base, "https://x.example/a"),
            "https://x.example/a"
        );
        assert_eq!(
            resolve_url("http://host.example", "a"),
            "http://host.example/a"
        );
        let i = index(&[("0.1.0", None, &["t"])]);
        assert_eq!(
            i.releases[0].archives[0].url,
            "http://host.example/k/download/v0.1.0/k-0.1.0-t.tar.gz"
        );
    }

    #[test]
    fn a_minor_or_stable_is_the_newest_release_never_yanked_or_pre() {
        let i = index(&[
            ("0.1.0", None, &["t"]),
            ("0.1.1", Some("broken"), &["t"]),
            ("0.1.2", None, &["other"]),
            ("0.2.0-rc.1", None, &["t"]),
            ("0.2.0-nightly.20261101", None, &["t"]),
            ("0.2.0-nightly.20261102", Some("broken"), &["t"]),
        ]);
        assert_eq!(chosen(&i, "0.1").as_deref(), Ok("0.1.0"));
        assert_eq!(chosen(&i, "stable").as_deref(), Ok("0.1.0"));
        assert_eq!(
            chosen(&i, "nightly").as_deref(),
            Ok("0.2.0-nightly.20261101")
        );
        // Named exactly, a yanked release and a candidate are installed.
        assert_eq!(chosen(&i, "0.1.1").as_deref(), Ok("0.1.1"));
        assert_eq!(chosen(&i, "=0.2.0-rc.1").as_deref(), Ok("0.2.0-rc.1"));
        let Err(e) = chosen(&i, "0.2") else {
            panic!("0.2 has no release")
        };
        assert_eq!(
            e,
            "the toolchain index has no k 0.2 for t; k 0.2.0-rc.1 is a pre-release, \
             installed only when named exactly (`k toolchain install 0.2.0-rc.1`)"
        );
        let Err(e) = chosen(&i, "0.1.2") else {
            panic!("no archive for t")
        };
        assert_eq!(e, "k 0.1.2 has no archive for t");
        let Err(e) = chosen(&i, "0.3.0") else {
            panic!("not listed")
        };
        assert_eq!(e, "k 0.3.0 is not in the toolchain index");
    }

    #[test]
    fn an_empty_choice_names_the_yanked_releases_that_would_fit() {
        let i = index(&[("0.1.0", Some("miscompiles"), &["t"])]);
        let Err(e) = chosen(&i, "0.1") else {
            panic!("only yanked")
        };
        assert_eq!(
            e,
            "the toolchain index has no k 0.1 for t; k 0.1.0 is yanked (miscompiles), \
             and is installed only when named exactly"
        );
    }

    #[test]
    fn with_only_a_candidate_the_choice_names_it() {
        // The 0.1.0-rc.1 dry run: an index holding only the candidate said
        // "has no a stable release".
        let i = index(&[
            ("0.1.0-rc.1", None, &["t"]),
            ("0.1.0-rc.2", None, &["t"]),
            ("0.1.0-nightly.20261101", None, &["t"]),
        ]);
        let hint = "; k 0.1.0-rc.2 is a pre-release, installed only when named exactly \
                    (`k toolchain install 0.1.0-rc.2`)";
        let Err(e) = chosen(&i, "stable") else {
            panic!("no release")
        };
        assert_eq!(
            e,
            format!("the toolchain index has no stable release for t{hint}")
        );
        let Err(e) = chosen(&i, "0.1") else {
            panic!("no release")
        };
        assert_eq!(e, format!("the toolchain index has no k 0.1 for t{hint}"));
        let Err(e) = chosen(&i, "0.2") else {
            panic!("no release")
        };
        assert_eq!(e, "the toolchain index has no k 0.2 for t");
        let i = index(&[]);
        let Err(e) = chosen(&i, "nightly") else {
            panic!("no nightly")
        };
        assert_eq!(e, "the toolchain index has no nightly for t");
    }

    #[test]
    fn specs_parse() {
        assert_eq!(Spec::parse("stable"), Some(Spec::Stable));
        assert_eq!(Spec::parse("nightly"), Some(Spec::Nightly));
        assert!(matches!(Spec::parse("0.3.1"), Some(Spec::Exact(_))));
        assert!(matches!(Spec::parse("0.3"), Some(Spec::Matching(..))));
        assert_eq!(Spec::parse("latest"), None);
        assert_eq!(Spec::parse("0.x"), None);
    }

    #[test]
    fn a_newer_or_broken_index_is_refused() {
        let Err(e) = parse(r#"{"format": 2, "releases": []}"#, "") else {
            panic!()
        };
        assert!(e.contains("format 2, newer than this k reads (1)"), "{e}");
        assert!(parse("{}", "").is_err());
        assert!(parse("not json", "").is_err());
        let bad = r#"{"format":1,"releases":[{"version":"0.1.0","archives":[{"target":"t","url":"u","sha256":"zz"}]}]}"#;
        assert!(parse(bad, "").is_err());
    }

    #[test]
    fn the_host_target_is_one_of_the_release_targets() {
        if let Some(t) = host_target() {
            assert!(
                [
                    "x86_64-unknown-linux-gnu",
                    "aarch64-unknown-linux-gnu",
                    "x86_64-apple-darwin",
                    "aarch64-apple-darwin",
                    "x86_64-pc-windows-msvc",
                    "aarch64-pc-windows-msvc",
                ]
                .contains(&t)
            );
        }
    }
}

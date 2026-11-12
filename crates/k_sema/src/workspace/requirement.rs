//! Toolchain versions and requirements: what `[package] k` and
//! `[workspace] k` say, and what `k.lock`'s `toolchain` records
//! (`docs/design/48-releases-and-versioning.md`, "The requirement and the
//! lock").
//!
//! A [`Version`] is `MAJOR.MINOR.PATCH`, optionally followed by a
//! pre-release (`0.6.0-rc.1`, `0.7.0-nightly.20261109`). A [`Requirement`]
//! has Cargo's caret meaning: `0.5` is `>=0.5.0, <0.6.0`, `1.4` is
//! `>=1.4.0, <2.0.0`, `0.5.2` is `>=0.5.2, <0.6.0`; `=0.5.2` is exactly that
//! version. A pre-release satisfies a caret requirement when its release
//! does, so a nightly or a release candidate of 0.7 satisfies `0.7`, and
//! nothing else.

use std::cmp::Ordering;
use std::fmt;

/// A toolchain version: `0.6.1`, `0.6.0-rc.1`, `0.7.0-nightly.20261109`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    /// The major version.
    pub major: u64,
    /// The minor version.
    pub minor: u64,
    /// The patch version.
    pub patch: u64,
    /// The pre-release after `-` (`rc.1`), if any.
    pub pre: Option<String>,
}

impl Version {
    /// Parses `MAJOR.MINOR.PATCH[-PRE]`. `None` for anything else: missing
    /// parts, leading zeros, build metadata, whitespace.
    ///
    /// ```
    /// use k_sema::workspace::Version;
    ///
    /// let v = Version::parse("0.7.0-nightly.20261109").unwrap();
    /// assert_eq!((v.major, v.minor, v.patch), (0, 7, 0));
    /// assert_eq!(v.pre.as_deref(), Some("nightly.20261109"));
    /// assert!(Version::parse("0.7").is_none());
    /// ```
    #[must_use]
    pub fn parse(text: &str) -> Option<Version> {
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let parts = numbers(core)?;
        let [major, minor, patch] = parts[..] else {
            return None;
        };
        if let Some(pre) = pre
            && !pre.split('.').all(|id| {
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
            pre: pre.map(str::to_owned),
        })
    }

    /// The version without its pre-release.
    #[must_use]
    pub fn release(&self) -> Version {
        Version {
            pre: None,
            ..self.clone()
        }
    }

    /// `true` when `other` is the same release line: equal major and
    /// minor, both without a pre-release. A toolchain that differs from
    /// the lock only so runs without delegating (patches never break).
    #[must_use]
    pub fn same_minor(&self, other: &Version) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.pre.is_none()
            && other.pre.is_none()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Semantic-versioning precedence: a pre-release comes before its
    /// release; pre-release identifiers compare numerically when both are
    /// numbers, else as text, and a shorter list first.
    fn cmp(&self, other: &Version) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => {
                    let mut a = a.split('.');
                    let mut b = b.split('.');
                    loop {
                        match (a.next(), b.next()) {
                            (None, None) => return Ordering::Equal,
                            (None, Some(_)) => return Ordering::Less,
                            (Some(_), None) => return Ordering::Greater,
                            (Some(x), Some(y)) => {
                                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                                    (Ok(x), Ok(y)) => x.cmp(&y),
                                    (Ok(_), Err(_)) => Ordering::Less,
                                    (Err(_), Ok(_)) => Ordering::Greater,
                                    (Err(_), Err(_)) => x.cmp(y),
                                };
                                if order != Ordering::Equal {
                                    return order;
                                }
                            }
                        }
                    }
                }
            })
    }
}

/// Dot-separated decimal numbers without leading zeros (one to three).
fn numbers(text: &str) -> Option<Vec<u64>> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    parts
        .iter()
        .map(|p| {
            let ok = !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'));
            if ok { p.parse().ok() } else { None }
        })
        .collect()
}

/// A toolchain requirement: `"0.5"`, `"1.4"`, `"0.5.2"` or `"=0.5.2"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// The text as written.
    text: String,
    /// `=`: exactly the given parts.
    exact: bool,
    /// The given parts: major, then minor and patch when written.
    parts: Vec<u64>,
    /// The pre-release of an exact requirement (`=0.6.0-rc.1`).
    pre: Option<String>,
}

impl Requirement {
    /// Parses a requirement. `None` for anything that isn't one: a range
    /// (`>=0.5`), a wildcard, a pre-release without `=`, whitespace.
    ///
    /// ```
    /// use k_sema::workspace::{Requirement, Version};
    ///
    /// let r = Requirement::parse("0.5").unwrap();
    /// let v = |s: &str| Version::parse(s).unwrap();
    /// assert!(r.matches(&v("0.5.9")));
    /// assert!(!r.matches(&v("0.6.0")));
    /// assert!(Requirement::parse("=0.5.2").unwrap().matches(&v("0.5.2")));
    /// assert!(Requirement::parse(">=0.5").is_none());
    /// ```
    #[must_use]
    pub fn parse(text: &str) -> Option<Requirement> {
        let (exact, rest) = match text.strip_prefix('=') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (core, pre) = match rest.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (rest, None),
        };
        let parts = numbers(core)?;
        if let Some(pre) = pre {
            // A pre-release is named exactly, with every part.
            if !exact || parts.len() != 3 {
                return None;
            }
            Version::parse(&format!("{core}-{pre}"))?;
        }
        Some(Requirement {
            text: text.to_owned(),
            exact,
            parts,
            pre: pre.map(str::to_owned),
        })
    }

    /// The requirement as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The lowest release it accepts: `0.5.0` for `0.5`, `0.5.2` for
    /// `=0.5.2` (an exact pre-release's release).
    ///
    /// ```
    /// use k_sema::workspace::Requirement;
    ///
    /// assert_eq!(Requirement::parse("0.5").unwrap().lowest().to_string(), "0.5.0");
    /// assert_eq!(Requirement::parse("=1.4.2").unwrap().lowest().to_string(), "1.4.2");
    /// ```
    #[must_use]
    pub fn lowest(&self) -> Version {
        self.lower()
    }

    /// `true` for an exact requirement (`=0.5.2`).
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.exact
    }

    /// The lowest release it accepts.
    fn lower(&self) -> Version {
        Version {
            major: self.parts[0],
            minor: self.parts.get(1).copied().unwrap_or(0),
            patch: self.parts.get(2).copied().unwrap_or(0),
            pre: None,
        }
    }

    /// The first release above what it accepts (exclusive).
    fn upper(&self) -> Version {
        let v = |major, minor, patch| Version {
            major,
            minor,
            patch,
            pre: None,
        };
        let major = self.parts[0];
        let minor = self.parts.get(1).copied();
        let patch = self.parts.get(2).copied();
        if self.exact {
            return match (minor, patch) {
                (None, _) => v(major + 1, 0, 0),
                (Some(m), None) => v(major, m + 1, 0),
                (Some(m), Some(p)) => v(major, m, p + 1),
            };
        }
        match (major, minor, patch) {
            (0, Some(0), Some(p)) => v(0, 0, p + 1),
            (0, Some(m), _) => v(0, m + 1, 0),
            (major, _, _) => v(major + 1, 0, 0),
        }
    }

    /// `true` when `version` satisfies the requirement.
    ///
    /// ```
    /// use k_sema::workspace::{Requirement, Version};
    ///
    /// let nightly = Version::parse("0.7.0-nightly.20261109").unwrap();
    /// assert!(Requirement::parse("0.7").unwrap().matches(&nightly));
    /// assert!(!Requirement::parse("0.6").unwrap().matches(&nightly));
    /// assert!(!Requirement::parse("=0.7.0").unwrap().matches(&nightly));
    /// ```
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        if self.exact && self.parts.len() == 3 {
            return version.major == self.parts[0]
                && version.minor == self.parts[1]
                && version.patch == self.parts[2]
                && version.pre == self.pre;
        }
        let release = version.release();
        if self.exact && version.pre.is_some() {
            return false;
        }
        self.lower() <= release && release < self.upper()
    }

    /// `true` when some version satisfies both requirements.
    #[must_use]
    pub fn intersects(&self, other: &Requirement) -> bool {
        for (a, b) in [(self, other), (other, self)] {
            if a.exact && a.parts.len() == 3 {
                let v = Version {
                    major: a.parts[0],
                    minor: a.parts[1],
                    patch: a.parts[2],
                    pre: a.pre.clone(),
                };
                return b.matches(&v);
            }
        }
        self.lower().max(other.lower()) < self.upper().min(other.upper())
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

// ------------------------------------------------------------- k.lock

/// The file name of the toolchain lock.
pub const TOOLCHAIN_LOCK: &str = "k.lock";

/// The toolchain `k.lock` records (`toolchain = "0.5.2"`), with the range
/// of its line; `Ok(None)` when the lock has no `toolchain` (a library's).
///
/// # Errors
///
/// A `K8002` when the text isn't TOML, `toolchain` isn't a version or
/// `format` isn't a format number, and a `K8202` when the lock is in a
/// format newer than this toolchain reads ([`crate::lockformat`]).
pub fn lock_toolchain(
    text: &str,
) -> Result<Option<(Version, k_diagnostics::TextRange)>, crate::lockformat::FormatError> {
    let invalid = |range, message| crate::lockformat::FormatError {
        code: k_diagnostics::codes::INVALID_LOCKFILE,
        range,
        message,
        help: None,
    };
    let table: toml::Table = toml::from_str(text).map_err(|e| {
        let offset = e.span().map_or(0, |s| s.start);
        invalid(
            super::line_at(text, offset),
            format!("k.lock can't be read: {}", e.message().trim()),
        )
    })?;
    crate::lockformat::read(TOOLCHAIN_LOCK, text)?;
    let Some(value) = table.get("toolchain") else {
        return Ok(None);
    };
    let range = super::key_line(text, "", "toolchain");
    match value.as_str().and_then(Version::parse) {
        Some(v) => Ok(Some((v, range))),
        None => Err(invalid(
            range,
            "`toolchain` in k.lock must be a toolchain version such as \"0.1.0\"; `k build` writes it"
                .to_owned(),
        )),
    }
}

/// The text of `k.lock` recording `version`: `old` (the current lock) with
/// its `toolchain` line replaced, or added after its comment header, or a
/// new lock when there is none. Everything else in `old` is kept, its
/// `format` included, so the lock stays in the format it was read in.
#[must_use]
pub fn with_lock_toolchain(old: Option<&str>, version: &Version) -> String {
    let line = format!("toolchain = \"{version}\"");
    let Some(old) = old else {
        return format!(
            "# k.lock: generated by `k build`; commit it. The toolchain that last built\n\
             # this project (docs/design/48-releases-and-versioning.md).\n\n{line}\n"
        );
    };
    let mut out = String::new();
    let mut replaced = false;
    let mut in_table = false;
    for l in old.split_inclusive('\n') {
        let t = l.trim();
        in_table |= t.starts_with('[');
        if !replaced && !in_table && t.split('=').next().is_some_and(|k| k.trim() == "toolchain") {
            out.push_str(&line);
            out.push('\n');
            replaced = true;
        } else {
            out.push_str(l);
        }
    }
    if replaced {
        return out;
    }
    // After the leading comments and blank lines.
    let header: usize = old
        .split_inclusive('\n')
        .take_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
        .map(str::len)
        .sum();
    let (head, rest) = old.split_at(header);
    let mut out = head.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&line);
    out.push('\n');
    if !rest.is_empty() {
        out.push('\n');
        out.push_str(rest);
    }
    out
}

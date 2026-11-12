//! Toolchain versions as the release process writes them: `0.6.0`,
//! `0.6.0-rc.1`, `0.7.0-nightly.20261109`.

use std::fmt;

/// A version: `major.minor.patch` and an optional pre-release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The major number.
    pub major: u64,
    /// The minor number.
    pub minor: u64,
    /// The patch number.
    pub patch: u64,
    /// The pre-release (`rc.1`), if any.
    pub pre: Option<String>,
}

impl Version {
    /// Parses `0.6.0` or `0.6.0-rc.1`; `None` for anything else.
    ///
    /// ```
    /// let v = xtask::Version::parse("0.6.0-rc.1").unwrap();
    /// assert_eq!((v.minor, v.pre.as_deref()), (6, Some("rc.1")));
    /// assert!(xtask::Version::parse("0.6").is_none());
    /// ```
    #[must_use]
    pub fn parse(text: &str) -> Option<Version> {
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let parts: Vec<u64> = core
            .split('.')
            .map(|p| {
                let ok = !p.is_empty()
                    && p.bytes().all(|b| b.is_ascii_digit())
                    && (p == "0" || !p.starts_with('0'));
                if ok { p.parse().ok() } else { None }
            })
            .collect::<Option<_>>()?;
        let [major, minor, patch] = parts[..] else {
            return None;
        };
        if let Some(pre) = pre
            && (pre.is_empty()
                || !pre.split('.').all(|p| {
                    !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                }))
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

    /// `major.minor`: what a project requires (`k = "0.6"`).
    #[must_use]
    pub fn minor_text(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// The release this is, or is a pre-release of: `0.6.0` for
    /// `0.6.0-rc.1`.
    #[must_use]
    pub fn release_text(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// `true` for a patch release (or a pre-release of one): `0.6.1`.
    #[must_use]
    pub fn is_patch(&self) -> bool {
        self.patch > 0
    }

    /// `true` for a pre-release: a release candidate or a nightly.
    #[must_use]
    pub fn is_pre(&self) -> bool {
        self.pre.is_some()
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

/// Today's date in UTC, `YYYY-MM-DD`.
#[must_use]
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The proleptic Gregorian date of `days` after 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `true` for a `YYYY-MM-DD` date.
#[must_use]
pub fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_404), (2025, 11, 12));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert!(is_date(&today()));
        assert!(!is_date("2026-1-12"));
    }

    #[test]
    fn versions_round_trip_and_reject_ranges() {
        for text in ["0.1.0", "1.4.2", "0.7.0-nightly.20261109", "0.6.0-rc.1"] {
            assert_eq!(
                Version::parse(text).map(|v| v.to_string()).as_deref(),
                Some(text)
            );
        }
        for bad in [
            "0.1",
            "01.1.0",
            "0.1.0-",
            "0.1.0-rc..1",
            ">=0.1.0",
            "v0.1.0",
        ] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
        let v = Version::parse("0.6.1-rc.2").unwrap();
        assert!(v.is_patch() && v.is_pre());
        assert_eq!(
            (v.minor_text(), v.release_text()),
            ("0.6".to_owned(), "0.6.1".to_owned())
        );
    }
}

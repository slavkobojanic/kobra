//! The format number every lockfile carries
//! (`docs/design/48-releases-and-versioning.md`, "Lockfile format
//! versions"; the procedure for changing one is
//! `docs/compiler/release.md` §4).
//!
//! A lockfile may say `format = N` at its top level, after its comment
//! header. Without the key it is format 1, which is what every lockfile
//! has been so far. Each lockfile has its own number ([`current`]): the
//! newest format of that file this toolchain reads and writes.
//!
//! * **Reading** ([`read`]): a missing `format` is 1; a positive whole
//!   number up to the current format is that format; a newer one is
//!   **K8202** at the `format` line, which names the format and this
//!   toolchain and is reported instead of whatever else the file's reader
//!   would say about it; anything else is **K8002**. Text that isn't TOML
//!   is left to the file's own reader (it reports K8002).
//! * **Writing** ([`LockFormat::stamp`]): a lockfile is written in the
//!   format it was read in. Format 1 is written without the key, as every
//!   earlier toolchain wrote it, unless the lock read declared
//!   `format = 1`; any later format is always written with it. Only
//!   `k upgrade` moves a lock to a newer format.
//!
//! A lock in a format too new to read is never rewritten: the writers
//! return no new text for it.

use k_diagnostics::{Code, Diagnostic, FileId, Span, TextRange, TextSize, codes};

/// The current format of each lockfile, by file name.
pub const FORMATS: &[(&str, u32)] = &[
    ("k.lock", 1),
    ("routes.lock", 1),
    ("rpc.lock", 1),
    ("schema.lock", 1),
    ("storage.lock", 1),
];

/// The newest format of the lockfile `file` (`rpc.lock`, …) this
/// toolchain reads and writes; `None` for a name that isn't a lockfile
/// with a format.
///
/// # Examples
///
/// ```
/// assert_eq!(k_sema::lockformat::current("rpc.lock"), Some(1));
/// assert_eq!(k_sema::lockformat::current("k.toml"), None);
/// ```
#[must_use]
pub fn current(file: &str) -> Option<u32> {
    FORMATS.iter().find(|(f, _)| *f == file).map(|(_, n)| *n)
}

/// The format a lockfile was read in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LockFormat {
    /// The format number.
    pub number: u32,
    /// Whether the lock says `format = N`; `false` for a lock without the
    /// key, which is format 1.
    pub declared: bool,
}

impl Default for LockFormat {
    fn default() -> LockFormat {
        LockFormat::IMPLIED
    }
}

impl LockFormat {
    /// The format of a lockfile without a `format` key, and of a new one.
    pub const IMPLIED: LockFormat = LockFormat {
        number: 1,
        declared: false,
    };

    /// `text`, a lock written by its writer (which never writes `format`),
    /// in this format: unchanged for [`LockFormat::IMPLIED`] or a text that
    /// already has a top-level `format` line, else with `format = N`
    /// inserted after the comment header (and the blank lines after it).
    ///
    /// # Examples
    ///
    /// ```
    /// use k_sema::lockformat::LockFormat;
    ///
    /// let lock = "# Generated.\n\n[pages.home]\nurl = \"/\"\n";
    /// assert_eq!(LockFormat::IMPLIED.stamp(lock), lock);
    /// let one = LockFormat { number: 1, declared: true };
    /// assert_eq!(one.stamp(lock), "# Generated.\n\nformat = 1\n\n[pages.home]\nurl = \"/\"\n");
    /// ```
    #[must_use]
    pub fn stamp(self, text: &str) -> String {
        if self == LockFormat::IMPLIED || format_line(text).is_some() {
            return text.to_owned();
        }
        let header: usize = text
            .split_inclusive('\n')
            .take_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
            .map(str::len)
            .sum();
        let (head, rest) = text.split_at(header);
        let mut out = head.to_owned();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("format = {}\n", self.number));
        if rest.starts_with('[') {
            out.push('\n');
        }
        out.push_str(rest);
        out
    }
}

/// Why a lockfile's format can't be used.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FormatError {
    /// `K8202` for a format newer than this toolchain's, `K8002` for a
    /// `format` that isn't a format number.
    pub code: Code,
    /// The `format` line.
    pub range: TextRange,
    /// What is wrong, naming the file.
    pub message: String,
    /// What resolves it, for `K8202`.
    pub help: Option<String>,
}

impl FormatError {
    /// The diagnostic, in `file`.
    #[must_use]
    pub fn diagnostic(&self, file: FileId) -> Diagnostic {
        let d = Diagnostic::new(self.code, Span::new(file, self.range), self.message.clone());
        match &self.help {
            Some(help) => d.with_help(help.clone()),
            None => d,
        }
    }
}

/// The format of lockfile `file` (its name, `rpc.lock`) from its `text`.
///
/// # Errors
///
/// A [`FormatError`] for a format newer than [`current`] (K8202) and for a
/// `format` that isn't a positive whole number (K8002).
///
/// # Examples
///
/// ```
/// use k_sema::lockformat::{LockFormat, read};
///
/// assert_eq!(read("rpc.lock", "# rpc.lock\n"), Ok(LockFormat::IMPLIED));
/// assert_eq!(read("rpc.lock", "format = 1\n").map(|f| f.declared), Ok(true));
/// let newer = read("rpc.lock", "format = 2\n").unwrap_err();
/// assert_eq!(newer.code.to_string(), "K8202");
/// ```
pub fn read(file: &str, text: &str) -> Result<LockFormat, FormatError> {
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        return Ok(LockFormat::IMPLIED);
    };
    let Some(value) = table.get("format") else {
        return Ok(LockFormat::IMPLIED);
    };
    let range = format_line(text).unwrap_or_else(|| TextRange::empty(TextSize::from(0)));
    let known = current(file).unwrap_or(1);
    let number = value
        .as_integer()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= 1);
    match number {
        Some(n) if n <= known => Ok(LockFormat {
            number: n,
            declared: true,
        }),
        Some(n) => Err(FormatError {
            code: codes::LOCKFILE_TOO_NEW,
            range,
            message: format!(
                "{file} is in format {n}, and k {} reads {file} formats up to {known}",
                env!("CARGO_PKG_VERSION")
            ),
            help: Some(format!(
                "a newer toolchain wrote this {file}: build with the toolchain k.lock names \
                 (`k toolchain install` installs it), or restore {file} from version control"
            )),
        }),
        None => Err(FormatError {
            code: codes::INVALID_LOCKFILE,
            range,
            message: format!(
                "{file} can't be read: `format` must be a format number such as 1; `k build` writes it"
            ),
            help: None,
        }),
    }
}

/// The range of the top-level `format = …` line, if the text has one.
fn format_line(text: &str) -> Option<TextRange> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            return None;
        }
        if let Some(rest) = t.strip_prefix("format")
            && rest.trim_start().starts_with('=')
        {
            let size = |n: usize| TextSize::from(u32::try_from(n).unwrap_or(u32::MAX));
            let body = line.trim_end_matches(['\n', '\r']);
            return Some(TextRange::new(size(offset), size(offset + body.len())));
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_line_is_found_before_the_first_table_only() {
        let text = "# h\nformat = 1\n[a]\nformat = 2\n";
        assert_eq!(
            format_line(text),
            Some(TextRange::new(TextSize::from(4), TextSize::from(14)))
        );
        assert_eq!(format_line("[a]\nformat = 2\n"), None);
        assert_eq!(format_line("formats = 2\n"), None);
    }

    #[test]
    fn a_format_that_isnt_a_number_is_invalid() {
        for bad in [
            "format = \"1\"\n",
            "format = 0\n",
            "format = -3\n",
            "format = 1.5\n",
        ] {
            let e = read("routes.lock", bad).unwrap_err();
            assert_eq!(e.code, codes::INVALID_LOCKFILE, "{bad}");
            assert_eq!(u32::from(e.range.start()), 0);
        }
    }

    #[test]
    fn stamping_keeps_a_declared_line_and_never_duplicates_it() {
        let two = LockFormat {
            number: 2,
            declared: true,
        };
        assert_eq!(two.stamp("format = 2\nx = 1\n"), "format = 2\nx = 1\n");
        assert_eq!(two.stamp(""), "format = 2\n");
        assert_eq!(two.stamp("# a\nx = 1\n"), "# a\nformat = 2\nx = 1\n");
        let one = LockFormat {
            number: 1,
            declared: true,
        };
        let stamped = one.stamp("# a\n# b\n\ntoolchain = \"0.1.0\"\n");
        assert_eq!(stamped, "# a\n# b\n\nformat = 1\ntoolchain = \"0.1.0\"\n");
        assert_eq!(read("k.lock", &stamped), Ok(one));
    }
}

//! Configuration from the environment (`docs/design/25-config-and-secrets.md`).
//!
//! The compiler lists every `@Env` field reachable from the production
//! dependency graph in the build manifest ([`ConfigField`]); at startup,
//! before a port or a database connection is opened, [`resolve`] reads all
//! of them at once and returns either every value or every problem.
//!
//! For each field: `NAME_FILE` (secrets only; one trailing line ending
//! removed), then `NAME`, then the default; setting both `NAME` and
//! `NAME_FILE` is a problem, as is `NAME_FILE` for a non-secret. A secret's
//! value never appears in a [`Problem`] or a `Debug` output.

use std::collections::BTreeMap;
use std::fmt;

/// The value kinds a config field can have (`ConfigValue` in the stdlib).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConfigKind {
    /// `String`, and `Url`/`EmailAddress` before their own validation.
    String,
    /// `Int`: a decimal integer in the safe-integer range.
    Int,
    /// `Double`.
    Double,
    /// `Bool`: exactly `true` or `false`.
    Bool,
    /// `Duration`: `300ms`, `30s`, `5m`, `2h`, `1d`, `10.5s`; stored in
    /// milliseconds.
    Duration,
    /// An enum without payloads, by case name.
    Enum(Vec<String>),
    /// `List<T>`, comma-separated.
    List(Box<ConfigKind>),
    /// A `k.crypto` key type, by name (`Ed25519PrivateKey`): a JWK, a PEM
    /// or base64url of the raw form, whose shape startup checks
    /// ([`crate::crypto::config_key_problem`]); the runtime parses it.
    Key(String),
    /// An `Int` constrained by `@Range(min...max)`: parsed as
    /// [`ConfigKind::Int`], then checked against the closed range.
    IntRange(i64, i64),
    /// A value constrained by `@Matches(pattern, message:)`: parsed as
    /// `kind`, then its text must match `pattern` (a K regular expression,
    /// checked with Rust's `regex` syntax; a pattern that syntax can't
    /// compile is not checked at startup). The problem is `message` when
    /// the annotation gives one, else `doesn't match <pattern>`.
    Matching {
        /// The kind the text is parsed as first.
        kind: Box<ConfigKind>,
        /// The pattern.
        pattern: String,
        /// The annotation's `message:`.
        message: Option<String>,
    },
}

impl ConfigKind {
    /// The kind of a config field from its K type as the build manifest
    /// writes it (`Int`, `Duration`, `List<String>`, `Secret<String>`,
    /// `Mode?`), the case names when it is an enum, and its `@Range`
    /// bounds. `Secret<T>` and `T?` are unwrapped (secrecy and optionality
    /// are separate fields of [`ConfigField`]). Returns `None` for a type
    /// the runtime can't read from text.
    #[must_use]
    pub fn from_manifest(
        ty: &str,
        cases: &[String],
        range: Option<(i64, i64)>,
    ) -> Option<ConfigKind> {
        let mut t = ty.trim();
        if let Some(inner) = t.strip_suffix('?') {
            t = inner;
        }
        if let Some(inner) = t.strip_prefix("Secret<").and_then(|r| r.strip_suffix('>')) {
            return ConfigKind::from_manifest(inner, cases, range);
        }
        if let Some(inner) = t.strip_prefix("List<").and_then(|r| r.strip_suffix('>')) {
            return ConfigKind::from_manifest(inner, cases, None)
                .map(|k| ConfigKind::List(Box::new(k)));
        }
        Some(match t {
            "String" | "Url" | "EmailAddress" => ConfigKind::String,
            "Int" => match range {
                Some((lo, hi)) => ConfigKind::IntRange(lo, hi),
                None => ConfigKind::Int,
            },
            "Double" => ConfigKind::Double,
            "Bool" => ConfigKind::Bool,
            "Duration" => ConfigKind::Duration,
            _ if !cases.is_empty() => ConfigKind::Enum(cases.to_vec()),
            _ if crate::crypto::is_config_key_type(t.rsplit('.').next().unwrap_or(t)) => {
                ConfigKind::Key(t.rsplit('.').next().unwrap_or(t).to_owned())
            }
            _ => return None,
        })
    }
}

/// One `@Env` field of a `config` declaration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConfigField {
    /// The environment variable, `STRIPE_WEBHOOK_SECRET`.
    pub env: String,
    /// Where it is declared, `payments.StripeWebhookConfig.signingSecret`,
    /// for messages.
    pub declared_at: String,
    /// The value kind (of the revealed value, for a secret).
    pub kind: ConfigKind,
    /// `Secret<T>`: may come from `NAME_FILE`; never printed.
    pub secret: bool,
    /// `T?`: missing means `nil`.
    pub optional: bool,
    /// The default, as environment text (`5m`), if the field has one.
    pub default: Option<String>,
}

impl ConfigField {
    /// A field from a build manifest entry (`k_codegen::ConfigEntry`):
    /// its variable, where it is declared, its K type as text, secrecy,
    /// whether it is required, its default as environment text, its enum
    /// case names and `@Range` bounds. A field that isn't required and has
    /// no default is optional (missing means `nil`). Returns `None` for a
    /// type the runtime can't read from text.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_manifest(
        env: &str,
        declared_at: &str,
        ty: &str,
        secret: bool,
        required: bool,
        default: Option<&str>,
        cases: &[String],
        range: Option<(i64, i64)>,
    ) -> Option<ConfigField> {
        Some(ConfigField {
            env: env.to_owned(),
            declared_at: declared_at.to_owned(),
            kind: ConfigKind::from_manifest(ty, cases, range)?,
            secret,
            optional: !required && default.is_none(),
            default: default.map(str::to_owned),
        })
    }
}

impl ConfigField {
    /// The field with its `@Matches` constraint (`pattern`, and the
    /// annotation's `message:`), from the build manifest; unchanged for
    /// `None`.
    #[must_use]
    pub fn with_pattern(mut self, pattern: Option<&str>, message: Option<&str>) -> ConfigField {
        if let Some(pattern) = pattern {
            self.kind = ConfigKind::Matching {
                kind: Box::new(self.kind),
                pattern: pattern.to_owned(),
                message: message.map(str::to_owned),
            };
        }
        self
    }
}

/// A parsed value.
#[derive(Clone, PartialEq)]
pub enum ConfigValue {
    /// Text.
    String(String),
    /// An integer.
    Int(i64),
    /// A finite floating-point number (`Double`).
    Double(f64),
    /// A boolean.
    Bool(bool),
    /// A duration in milliseconds.
    Duration(f64),
    /// An enum case name.
    Enum(String),
    /// A `List<…>` value: the comma-separated items, each parsed as the
    /// element kind.
    List(Vec<ConfigValue>),
    /// `nil` for a missing optional field.
    Nil,
    /// A secret's value. Its `Debug` output is `Secret(•••)`.
    Secret(Box<ConfigValue>),
}

impl fmt::Debug for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigValue::String(s) => write!(f, "{s:?}"),
            ConfigValue::Int(i) => write!(f, "{i}"),
            ConfigValue::Double(d) => write!(f, "{d}"),
            ConfigValue::Bool(b) => write!(f, "{b}"),
            ConfigValue::Duration(ms) => write!(f, "{ms}ms"),
            ConfigValue::Enum(c) => write!(f, ".{c}"),
            ConfigValue::List(l) => f.debug_list().entries(l).finish(),
            ConfigValue::Nil => write!(f, "nil"),
            ConfigValue::Secret(_) => write!(f, "Secret(•••)"),
        }
    }
}

impl ConfigValue {
    /// The value as JavaScript receives it through `op_k_server_info`:
    /// text, numbers (a `Duration` in milliseconds), booleans, an enum's
    /// case name, arrays, `null` for `nil`, and a secret as
    /// `{"$secret": value}`, which generated code wraps in `Secret` at once.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            ConfigValue::String(s) | ConfigValue::Enum(s) => Value::String(s.clone()),
            ConfigValue::Int(i) => Value::from(*i),
            ConfigValue::Double(d) | ConfigValue::Duration(d) => {
                serde_json::Number::from_f64(*d).map_or(Value::Null, Value::Number)
            }
            ConfigValue::Bool(b) => Value::Bool(*b),
            ConfigValue::List(l) => Value::Array(l.iter().map(ConfigValue::to_json).collect()),
            ConfigValue::Nil => Value::Null,
            ConfigValue::Secret(v) => serde_json::json!({ "$secret": v.to_json() }),
        }
    }
}

/// The development environment of `k run`, `k dev` and the end-to-end
/// tests (25-config-and-secrets.md, "Per environment").
///
/// It has the variables of an optional `.env` file at the project root, under the process
/// environment (which wins on a conflict) when `with_process` is set.
/// `NAME_FILE` paths that are relative are read from the project root.
#[derive(Clone, Debug, Default)]
pub struct DevEnvironment {
    /// The project root.
    pub root: std::path::PathBuf,
    /// The `.env` variables, by name; a name set twice keeps its last value.
    pub dotenv: BTreeMap<String, String>,
    /// Whether the process environment is consulted first.
    pub with_process: bool,
}

impl DevEnvironment {
    /// Reads `root/.env` if it exists (a missing file is no variables).
    #[must_use]
    pub fn load(root: &std::path::Path, with_process: bool) -> DevEnvironment {
        let text = std::fs::read_to_string(root.join(".env")).unwrap_or_default();
        DevEnvironment {
            root: root.to_path_buf(),
            dotenv: parse_dotenv(&text),
            with_process,
        }
    }

    /// The `.env` keys the process environment shadows: those it also sets,
    /// to another value, in name order (`k dev` names them;
    /// 25-config-and-secrets.md, "Per environment"). Empty without
    /// [`DevEnvironment::with_process`].
    #[must_use]
    pub fn shadowed(&self) -> Vec<String> {
        self.shadowed_by(|name| std::env::var(name).ok())
    }

    /// [`DevEnvironment::shadowed`], with `process` standing for the
    /// process environment.
    #[must_use]
    pub fn shadowed_by(&self, process: impl Fn(&str) -> Option<String>) -> Vec<String> {
        if !self.with_process {
            return Vec::new();
        }
        self.dotenv
            .iter()
            .filter(|(name, value)| process(name).is_some_and(|p| &p != *value))
            .map(|(name, _)| name.clone())
            .collect()
    }
}

/// Parses `.env` text into its variables, by name.
///
/// The text is `NAME=value` lines; blank lines and lines starting with
/// `#` are skipped, `export ` before a name is allowed, and a value
/// wrapped in matching single or double quotes loses them. Lines without
/// `=` are ignored.
#[must_use]
pub fn parse_dotenv(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let unquoted = ['"', '\'']
            .iter()
            .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
            .unwrap_or(value);
        out.insert(name.trim().to_owned(), unquoted.to_owned());
    }
    out
}

impl Environment for DevEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        if self.with_process {
            if let Ok(v) = std::env::var(name) {
                return Some(v);
            }
        }
        self.dotenv.get(name).cloned()
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        let p = std::path::Path::new(path);
        let full = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        };
        std::fs::read_to_string(full).map_err(|e| e.to_string())
    }
}

/// One problem with the configuration, printed as one line of the startup
/// report.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Problem {
    /// The variable.
    pub env: String,
    /// What is wrong. Never contains a secret's value.
    pub message: String,
}

/// Where values come from: the process environment, or a test's map.
pub trait Environment {
    /// The variable's value, if set.
    fn var(&self, name: &str) -> Option<String>;
    /// The contents of the file at `path`.
    ///
    /// # Errors
    ///
    /// The I/O error, as text.
    fn read_file(&self, path: &str) -> Result<String, String>;
}

/// The real process environment and file system.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
}

/// Parses a `Duration` configuration value, a decimal number with one of
/// the units `ms`, `s`, `m`, `h` or `d` (`250ms`, `1.5h`), to milliseconds.
/// Returns `None` for anything else: no unit, no digits, a sign, a leading
/// or trailing point, or more than one point.
fn parse_duration(text: &str) -> Option<f64> {
    let units = [
        ("ms", 1.0),
        ("s", 1000.0),
        ("m", 60_000.0),
        ("h", 3_600_000.0),
        ("d", 86_400_000.0),
    ];
    for (suffix, factor) in units {
        if let Some(number) = text.strip_suffix(suffix) {
            if number.is_empty() || number.starts_with('.') || number.ends_with('.') {
                return None;
            }
            if !number.chars().all(|c| c.is_ascii_digit() || c == '.')
                || number.matches('.').count() > 1
            {
                return None;
            }
            return number.parse::<f64>().ok().map(|n| n * factor);
        }
    }
    None
}

/// Parses the text of one variable as a value of `kind`.
///
/// # Errors
///
/// The end of a message that follows the variable's name (`is not an Int`,
/// `is outside 1...10`); it never contains the text itself, which may be a
/// secret.
fn parse(kind: &ConfigKind, text: &str) -> Result<ConfigValue, String> {
    match kind {
        ConfigKind::String => Ok(ConfigValue::String(text.to_owned())),
        ConfigKind::Matching {
            kind,
            pattern,
            message,
        } => {
            let value = parse(kind, text)?;
            match regex::Regex::new(pattern) {
                Ok(re) if !re.is_match(text) => Err(message
                    .clone()
                    .unwrap_or_else(|| format!("doesn't match {pattern}"))),
                _ => Ok(value),
            }
        }
        ConfigKind::IntRange(lo, hi) => match parse(&ConfigKind::Int, text)? {
            ConfigValue::Int(n) if (*lo..=*hi).contains(&n) => Ok(ConfigValue::Int(n)),
            _ => Err(format!("is outside {lo}...{hi}")),
        },
        ConfigKind::Int => text
            .parse::<i64>()
            .ok()
            .filter(|n| n.unsigned_abs() < (1u64 << 53))
            .map(ConfigValue::Int)
            .ok_or_else(|| "is not an Int".to_owned()),
        ConfigKind::Double => text
            .parse::<f64>()
            .ok()
            .filter(|d| d.is_finite())
            .map(ConfigValue::Double)
            .ok_or_else(|| "is not a Double".to_owned()),
        ConfigKind::Bool => match text {
            "true" => Ok(ConfigValue::Bool(true)),
            "false" => Ok(ConfigValue::Bool(false)),
            _ => Err("is not a Bool (expected true or false)".to_owned()),
        },
        ConfigKind::Duration => parse_duration(text)
            .map(ConfigValue::Duration)
            .ok_or_else(|| "is not a Duration (expected e.g. 300s, 5m)".to_owned()),
        ConfigKind::Enum(cases) => {
            if cases.iter().any(|c| c == text) {
                Ok(ConfigValue::Enum(text.to_owned()))
            } else {
                Err(format!("is not one of {}", cases.join(", ")))
            }
        }
        ConfigKind::Key(ty) => match crate::crypto::config_key_problem(ty, text) {
            None => Ok(ConfigValue::String(text.trim().to_owned())),
            Some(problem) => Err(problem),
        },
        ConfigKind::List(inner) => {
            if text.is_empty() {
                return Ok(ConfigValue::List(Vec::new()));
            }
            text.split(',')
                .map(|item| parse(inner, item.trim()))
                .collect::<Result<Vec<_>, _>>()
                .map(ConfigValue::List)
        }
    }
}

/// Reads every field. Returns all values by variable name, or every
/// problem, sorted by variable.
///
/// # Errors
///
/// Every [`Problem`]: missing required values, malformed values, both
/// `NAME` and `NAME_FILE` set, `NAME_FILE` on a non-secret, an unreadable
/// secret file.
pub fn resolve(
    fields: &[ConfigField],
    env: &dyn Environment,
) -> Result<BTreeMap<String, ConfigValue>, Vec<Problem>> {
    let mut values = BTreeMap::new();
    let mut problems = Vec::new();
    for field in fields {
        let file_var = format!("{}_FILE", field.env);
        let direct = env.var(&field.env);
        let file = env.var(&file_var);
        let shown = |raw: &str| {
            if field.secret {
                "Secret(•••)".to_owned()
            } else {
                format!("{raw:?}")
            }
        };
        let problem = |message: String| Problem {
            env: field.env.clone(),
            message,
        };
        let raw = match (direct, file) {
            (Some(_), Some(_)) => {
                problems.push(problem(format!(
                    "both {} and {file_var} are set; set only one",
                    field.env
                )));
                continue;
            }
            (None, Some(path)) if !field.secret => {
                problems.push(problem(format!(
                    "{file_var} is set, but {} isn't a secret; set {} instead",
                    field.declared_at, field.env
                )));
                let _ = path;
                continue;
            }
            (None, Some(path)) => match env.read_file(&path) {
                Ok(mut text) => {
                    if text.ends_with("\r\n") {
                        text.truncate(text.len() - 2);
                    } else if text.ends_with('\n') {
                        text.truncate(text.len() - 1);
                    }
                    Some(text)
                }
                Err(e) => {
                    problems.push(problem(format!(
                        "{file_var} names a file that can't be read: {e}"
                    )));
                    continue;
                }
            },
            (Some(v), None) => Some(v),
            (None, None) => field.default.clone(),
        };
        let Some(raw) = raw else {
            if field.optional {
                values.insert(field.env.clone(), ConfigValue::Nil);
            } else {
                let how = if field.secret {
                    format!("set {} or {file_var}", field.env)
                } else {
                    format!("set {}", field.env)
                };
                problems.push(problem(format!(
                    "missing (required by {}; {how})",
                    field.declared_at
                )));
            }
            continue;
        };
        match parse(&field.kind, &raw) {
            Ok(v) if field.secret => {
                values.insert(field.env.clone(), ConfigValue::Secret(Box::new(v)));
            }
            Ok(v) => {
                values.insert(field.env.clone(), v);
            }
            Err(e) => problems.push(problem(format!("{} {e}", shown(&raw)))),
        }
    }
    if problems.is_empty() {
        Ok(values)
    } else {
        problems.sort_by(|a, b| a.env.cmp(&b.env));
        Err(problems)
    }
}

/// Formats problems as the startup report printed before exiting with
/// status 78.
#[must_use]
pub fn report(problems: &[Problem]) -> String {
    let width = problems.iter().map(|p| p.env.len()).max().unwrap_or(0);
    let mut out = format!(
        "error: configuration is invalid ({} problem{})\n",
        problems.len(),
        if problems.len() == 1 { "" } else { "s" }
    );
    for p in problems {
        out.push_str(&format!("  {:width$}  {}\n", p.env, p.message));
    }
    out
}

/// The exit status for invalid configuration (`EX_CONFIG`).
pub const EXIT_CONFIG: i32 = 78;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadowed_names_the_dotenv_keys_the_process_overrides_with_another_value() {
        let env = DevEnvironment {
            root: std::path::PathBuf::new(),
            dotenv: parse_dotenv("B=file\nA=file\nSAME=v\nONLY=x\n"),
            with_process: true,
        };
        let process = |name: &str| match name {
            "A" | "B" => Some("process".to_owned()),
            "SAME" => Some("v".to_owned()),
            _ => None,
        };
        assert_eq!(env.shadowed_by(process), vec!["A", "B"]);
        let file_only = DevEnvironment {
            with_process: false,
            ..env
        };
        assert!(file_only.shadowed_by(process).is_empty());
    }

    #[test]
    fn manifest_types_map_to_kinds() {
        let none: &[String] = &[];
        assert_eq!(
            ConfigKind::from_manifest("Int", none, None),
            Some(ConfigKind::Int)
        );
        assert_eq!(
            ConfigKind::from_manifest("Int", none, Some((1, 10))),
            Some(ConfigKind::IntRange(1, 10))
        );
        assert_eq!(
            ConfigKind::from_manifest("Secret<String>?", none, None),
            Some(ConfigKind::String)
        );
        assert_eq!(
            ConfigKind::from_manifest("List<Duration>", none, None),
            Some(ConfigKind::List(Box::new(ConfigKind::Duration)))
        );
        let cases = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            ConfigKind::from_manifest("Mode", &cases, None),
            Some(ConfigKind::Enum(cases.clone()))
        );
        assert_eq!(ConfigKind::from_manifest("Mystery", none, None), None);
        assert_eq!(
            ConfigKind::from_manifest("Secret<Ed25519PrivateKey>", none, None),
            Some(ConfigKind::Key("Ed25519PrivateKey".to_owned()))
        );
        assert_eq!(ConfigKind::from_manifest("", none, None), None);
    }

    #[test]
    fn ranges_are_checked_after_parsing() {
        let k = ConfigKind::IntRange(1, 10);
        assert_eq!(parse(&k, "10"), Ok(ConfigValue::Int(10)));
        assert!(parse(&k, "11").is_err_and(|e| e.contains("1...10")));
        assert!(parse(&k, "x").is_err());
    }

    #[test]
    fn values_cross_to_javascript_as_json_with_secrets_marked() {
        let v = ConfigValue::List(vec![ConfigValue::Duration(1500.0), ConfigValue::Nil]);
        assert_eq!(v.to_json(), serde_json::json!([1500.0, null]));
        let s = ConfigValue::Secret(Box::new(ConfigValue::String("x".into())));
        assert_eq!(s.to_json(), serde_json::json!({"$secret": "x"}));
        assert_eq!(format!("{s:?}"), "Secret(•••)");
    }

    #[test]
    fn dotenv_lines_are_parsed_leniently() {
        let env = parse_dotenv(
            "# c\n\nA=1\nexport B = two \nC=\"quoted value\"\nD='x'\nnot a pair\nE=a=b\n",
        );
        assert_eq!(env.get("A").map(String::as_str), Some("1"));
        assert_eq!(env.get("B").map(String::as_str), Some("two"));
        assert_eq!(env.get("C").map(String::as_str), Some("quoted value"));
        assert_eq!(env.get("D").map(String::as_str), Some("x"));
        assert_eq!(env.get("E").map(String::as_str), Some("a=b"));
        assert_eq!(env.len(), 5);
    }

    #[test]
    fn the_dev_environment_reads_files_relative_to_the_root() {
        let root = std::env::temp_dir().join(format!("k_dev_env_{}", std::process::id()));
        std::fs::create_dir_all(root.join("secrets")).expect("dir");
        std::fs::write(root.join(".env"), "TOKEN_FILE=secrets/t\nPLAIN=p\n").expect("env");
        std::fs::write(root.join("secrets/t"), "s3cret\n").expect("secret");
        let env = DevEnvironment::load(&root, false);
        let fields = vec![
            ConfigField {
                env: "TOKEN".into(),
                declared_at: "a.C.token".into(),
                kind: ConfigKind::String,
                secret: true,
                optional: false,
                default: None,
            },
            ConfigField {
                env: "PLAIN".into(),
                declared_at: "a.C.plain".into(),
                kind: ConfigKind::String,
                secret: false,
                optional: false,
                default: None,
            },
        ];
        let values = resolve(&fields, &env).expect("resolves");
        assert_eq!(
            values["TOKEN"],
            ConfigValue::Secret(Box::new(ConfigValue::String("s3cret".into())))
        );
        assert_eq!(values["PLAIN"], ConfigValue::String("p".into()));
        assert!(
            DevEnvironment::load(&root.join("missing"), false)
                .dotenv
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

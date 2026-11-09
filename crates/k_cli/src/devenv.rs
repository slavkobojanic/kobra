//! The development environment of `k run`, `k dev` and `k migrate`
//! (`docs/design/25-config-and-secrets.md`, "Per environment", and
//! `docs/design/24-projects.md`, "Development settings").
//!
//! A development run needs no setup: what the process environment and the
//! project's `.env` leave unset comes, in this order, from
//!
//! 1. `[dev.env]` in the project's `k.toml` (committed development values;
//!    `{port}` and `{origin}` stand for the app's port and origin),
//! 2. `[dev.secrets]`, generated on first use into `.k/secrets/` (never
//!    committed),
//! 3. the toolchain's own development defaults: `DATABASE_URL` is a SQLite
//!    file `.k/<package>.db`, and an app that sends email over SMTP gets a
//!    local mail catcher ([`MailSink`]) whose messages are printed and kept
//!    in `.k/mail/`.
//!
//! A workspace's wiring (`k dev` at a root) sits above `.env`. Production
//! (`dist/app`) reads none of this.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use k_sema::workspace::{DevSecret, DevSettings};
use k_server::config::{ConfigField, DevEnvironment, Environment};

/// Prints `text` on standard error, each line after `label | ` when there
/// is a label (an app of a workspace run).
pub(crate) fn say(label: Option<&str>, text: &str) {
    for line in text.trim_end_matches('\n').lines() {
        match label {
            Some(l) => eprintln!("{l} | {line}"),
            None => eprintln!("{line}"),
        }
    }
}

/// The `[dev]` section of the `k.toml` in `project`; empty when there is
/// none or it can't be read (a malformed one fails the build first).
#[must_use]
pub(crate) fn settings(project: &Path) -> DevSettings {
    std::fs::read_to_string(project.join(k_sema::workspace::MANIFEST))
        .map(|text| {
            k_sema::workspace::parse_manifest("k.toml", &text, true, false)
                .0
                .dev
        })
        .unwrap_or_default()
}

/// What one app's development environment is assembled from.
pub(crate) struct DevInputs<'a> {
    /// The project directory.
    pub project: &'a Path,
    /// `[package] name`.
    pub package: &'a str,
    /// The port it serves on.
    pub port: u16,
    /// A workspace's wiring, above `.env`.
    pub wiring: &'a BTreeMap<String, String>,
}

/// An assembled development environment, and the notes to print about it
/// (never a secret's value, except a generated development password).
pub(crate) struct Prepared {
    /// The environment [`k_server::config::resolve`] reads.
    pub env: DevEnvironment,
    /// Lines to print before the app starts.
    pub notes: Vec<String>,
}

/// The `DATABASE_URL` of a development run that sets none.
#[must_use]
pub(crate) fn default_database_url(package: &str) -> String {
    format!("sqlite:.k/{package}.db")
}

/// Assembles the development environment for an app reading `fields`
/// (module docs). `mail` starts the app's mail catcher on first need and
/// gives its port.
///
/// # Errors
///
/// A problem to print before exiting with status 78: a `[dev.env]` value
/// for a secret (secrets are never committed; `[dev.secrets]` generates
/// them), or a development secret that can't be written.
pub(crate) fn prepare(
    inputs: &DevInputs<'_>,
    fields: &[ConfigField],
    mail: &mut dyn FnMut() -> Option<u16>,
) -> Result<Prepared, String> {
    let mut env = DevEnvironment::load(inputs.project, true);
    let mut notes = Vec::new();
    // Names only: a value may be a secret.
    let shadowed = env.shadowed();
    if !shadowed.is_empty() {
        notes.push(format!(
            "note: the process environment shadows these `.env` keys: {}",
            shadowed.join(", ")
        ));
    }
    env.dotenv.extend(inputs.wiring.clone());
    let field = |name: &str| fields.iter().find(|f| f.env == name);
    let is_set = |env: &DevEnvironment, name: &str| {
        env.var(name).is_some() || env.var(&format!("{name}_FILE")).is_some()
    };
    let dev = settings(inputs.project);
    let origin = format!("http://localhost:{}", inputs.port);
    for (name, value) in &dev.env {
        if field(name).is_some_and(|f| f.secret) {
            return Err(format!(
                "error: `[dev.env] {name}` in k.toml sets a secret, and secrets are never committed; \
                 generate it with `[dev.secrets] {name} = \"token\"` (or \"password-hash\") instead"
            ));
        }
        if !is_set(&env, name) {
            let value = value
                .replace("{port}", &inputs.port.to_string())
                .replace("{origin}", &origin);
            env.dotenv.insert(name.clone(), value);
        }
    }
    for (name, kind) in &dev.secrets {
        if field(name).is_none() || is_set(&env, name) {
            continue;
        }
        let (value, password) = secret(inputs.project, name, *kind)?;
        if let Some(password) = password {
            notes.push(format!(
                "note: {name} is the hash of the development password {password} (kept in {})",
                crate::sources::display(
                    &secrets_dir(inputs.project).join(format!("{name}.password"))
                )
            ));
        }
        env.dotenv.insert(name.clone(), value);
    }
    if field("DATABASE_URL").is_some() && !is_set(&env, "DATABASE_URL") {
        env.dotenv.insert(
            "DATABASE_URL".to_owned(),
            default_database_url(inputs.package),
        );
    }
    if field("SMTP_HOST").is_some()
        && !is_set(&env, "SMTP_HOST")
        && let Some(port) = mail()
    {
        for (name, value) in [
            ("SMTP_HOST", "127.0.0.1".to_owned()),
            ("SMTP_PORT", port.to_string()),
            ("SMTP_SECURITY", "none".to_owned()),
        ] {
            env.dotenv.insert(name.to_owned(), value);
        }
        if field("MAIL_FROM").is_some() && !is_set(&env, "MAIL_FROM") {
            env.dotenv
                .insert("MAIL_FROM".to_owned(), "noreply@example.com".to_owned());
        }
        notes.push(format!(
            "note: email goes to the development mail catcher: each message is printed and kept in {}",
            crate::sources::display(&inputs.project.join(".k").join("mail"))
        ));
    }
    Ok(Prepared { env, notes })
}

/// The directory of a project's generated development secrets.
fn secrets_dir(project: &Path) -> PathBuf {
    project.join(".k").join("secrets")
}

/// The development secret `name` of `kind`: read from `.k/secrets/<name>`,
/// or generated there on first use. A password hash also returns the
/// password, kept in `.k/secrets/<name>.password`.
///
/// # Errors
///
/// The secret can't be generated, read or written.
fn secret(project: &Path, name: &str, kind: DevSecret) -> Result<(String, Option<String>), String> {
    let dir = secrets_dir(project);
    let path = dir.join(name);
    let password_path = dir.join(format!("{name}.password"));
    let problem = |e: String| format!("error: development secret {name}: {e}");
    if let Ok(value) = std::fs::read_to_string(&path)
        && !value.trim().is_empty()
    {
        let password = match kind {
            DevSecret::Token => None,
            DevSecret::PasswordHash => std::fs::read_to_string(&password_path).ok(),
        };
        return Ok((value.trim().to_owned(), password));
    }
    std::fs::create_dir_all(&dir).map_err(|e| problem(e.to_string()))?;
    let token = k_server::process::random_token().map_err(problem)?;
    let (value, password) = match kind {
        DevSecret::Token => (token, None),
        DevSecret::PasswordHash => {
            let password: String = token.chars().take(16).collect();
            let hash = k_server::ops::password_hash(&password).map_err(problem)?;
            std::fs::write(&password_path, &password).map_err(|e| problem(e.to_string()))?;
            (hash, Some(password))
        }
    };
    std::fs::write(&path, &value).map_err(|e| problem(e.to_string()))?;
    Ok((value, password))
}

/// Runs `[dev] seed` on a development database that this start created:
/// opens it (applying its migrations), and when every migration of the
/// build was applied just now, runs the seed script. Returns what to print.
///
/// # Errors
///
/// The database can't be opened, or the script can't be read or fails
/// (its statements are rolled back).
pub(crate) fn seed(
    project: &Path,
    setup: &k_server::db::DatabaseSetup,
) -> Result<Vec<String>, String> {
    let Some(script) = settings(project).seed else {
        return Ok(Vec::new());
    };
    let total = setup.manifest.migrations.len();
    let (db, applied) =
        k_server::db::ProcessDatabase::open(setup.clone()).map_err(|e| format!("database: {e}"))?;
    let mut lines: Vec<String> = applied
        .iter()
        .map(|m| format!("applied migration {m}"))
        .collect();
    if applied.is_empty() || applied.len() != total {
        return Ok(lines);
    }
    let path = project.join(&script);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("[dev] seed `{}`: {e}", crate::sources::display(&path)))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let ran = runtime
        .block_on(db.run_script(&text))
        .map_err(|e| format!("[dev] seed `{}`: {e}", crate::sources::display(&path)))?;
    lines.push(format!(
        "seeded the new database from {} ({ran} statement{})",
        crate::sources::display(&path),
        if ran == 1 { "" } else { "s" }
    ));
    Ok(lines)
}

/// The development mail catcher of one app: a local SMTP server
/// ([`k_server::fakes::FakeSmtp`]) that accepts every message. Each one is
/// printed (sender, recipients, subject and the links in its body, so a
/// confirmation link is one click away) and kept as an `.eml` file in the
/// project's `.k/mail/`.
pub(crate) struct MailSink {
    /// The server.
    smtp: k_server::fakes::FakeSmtp,
    /// How many messages were reported.
    seen: usize,
    /// Where messages are kept.
    dir: PathBuf,
}

impl MailSink {
    /// Starts a catcher for the app in `project`.
    ///
    /// # Errors
    ///
    /// No local port could be bound.
    pub(crate) fn start(project: &Path) -> std::io::Result<MailSink> {
        Ok(MailSink {
            smtp: k_server::fakes::FakeSmtp::start()?,
            seen: 0,
            dir: project.join(".k").join("mail"),
        })
    }

    /// The SMTP port.
    pub(crate) fn port(&self) -> u16 {
        self.smtp.port()
    }

    /// Reports and keeps the messages received since the last call.
    pub(crate) fn poll(&mut self, label: Option<&str>) {
        let mail = self.smtp.mail();
        for m in mail.iter().skip(self.seen) {
            self.seen += 1;
            let _ = std::fs::create_dir_all(&self.dir);
            let file = self.dir.join(format!(
                "{}-{:03}.eml",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                self.seen
            ));
            let kept = std::fs::write(&file, &m.data).is_ok();
            let mut text = format!("mail from {} to {}: {}", m.from, m.to.join(", "), m.subject);
            for link in links(&m.data) {
                text.push_str(&format!("\n  {link}"));
            }
            if kept {
                text.push_str(&format!("\n  (kept in {})", crate::sources::display(&file)));
            }
            say(label, &text);
        }
    }
}

/// The distinct `http://` and `https://` links in a message, after
/// undoing quoted-printable soft line breaks and `=3D`.
fn links(data: &str) -> Vec<String> {
    let text = data
        .replace("=\r\n", "")
        .replace("=\n", "")
        .replace("=3D", "=");
    let mut out: Vec<String> = Vec::new();
    for start in text.match_indices("http").map(|(i, _)| i) {
        let rest = &text[start..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            continue;
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>' | '\'' | ')'))
            .unwrap_or(rest.len());
        let link = rest[..end].trim_end_matches(['.', ',']).to_owned();
        if !out.contains(&link) {
            out.push(link);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(env: &str, secret: bool) -> ConfigField {
        ConfigField {
            env: env.into(),
            declared_at: format!("app.C.{env}"),
            kind: k_server::config::ConfigKind::String,
            secret,
            optional: false,
            default: None,
        }
    }

    /// A project directory with `k.toml` holding `dev`, removed afterwards.
    struct Project(PathBuf);

    impl Project {
        fn new(tag: &str, dev: &str) -> Project {
            let dir = std::env::temp_dir().join(format!("k_devenv_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(
                dir.join("k.toml"),
                format!("[package]\nname = \"app\"\nk = \"0.1\"\n\n{dev}"),
            )
            .expect("k.toml");
            Project(dir)
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn prepared(p: &Project, fields: &[ConfigField]) -> Result<Prepared, String> {
        let wiring = BTreeMap::new();
        let inputs = DevInputs {
            project: &p.0,
            package: "app",
            port: 5301,
            wiring: &wiring,
        };
        prepare(&inputs, fields, &mut || Some(2525))
    }

    #[test]
    fn dev_values_defaults_and_generated_secrets_fill_what_is_unset() {
        let p = Project::new(
            "fill",
            "[dev.env]\nAPP_CALLBACK = \"http://127.0.0.1:{port}/cb\"\n\n[dev.secrets]\nAPP_TOKEN = \"token\"\nAPP_HASH = \"password-hash\"\n",
        );
        let fields = [
            field("APP_CALLBACK", false),
            field("APP_TOKEN", true),
            field("APP_HASH", true),
            field("DATABASE_URL", false),
            field("SMTP_HOST", false),
            field("MAIL_FROM", false),
        ];
        let first = prepared(&p, &fields).expect("prepared");
        let var = |p: &Prepared, n: &str| p.env.dotenv.get(n).cloned().unwrap_or_default();
        assert_eq!(var(&first, "APP_CALLBACK"), "http://127.0.0.1:5301/cb");
        assert_eq!(var(&first, "DATABASE_URL"), "sqlite:.k/app.db");
        assert_eq!(var(&first, "SMTP_HOST"), "127.0.0.1");
        assert_eq!(var(&first, "SMTP_PORT"), "2525");
        assert_eq!(var(&first, "SMTP_SECURITY"), "none");
        assert!(var(&first, "APP_HASH").starts_with("$argon2id$"));
        assert_eq!(var(&first, "APP_TOKEN").len(), 43);
        assert!(
            first
                .notes
                .iter()
                .any(|n| n.contains("development password")),
            "{:?}",
            first.notes
        );
        // Kept for the next start.
        let again = prepared(&p, &fields).expect("prepared");
        assert_eq!(var(&again, "APP_TOKEN"), var(&first, "APP_TOKEN"));
        assert_eq!(var(&again, "APP_HASH"), var(&first, "APP_HASH"));
        // `.env` wins over `[dev]`.
        std::fs::write(
            p.0.join(".env"),
            "APP_CALLBACK=from-env\nSMTP_HOST=mail.local\n",
        )
        .expect(".env");
        let with_env = prepared(&p, &fields).expect("prepared");
        assert_eq!(var(&with_env, "APP_CALLBACK"), "from-env");
        assert_eq!(var(&with_env, "SMTP_HOST"), "mail.local");
        assert_eq!(
            var(&with_env, "SMTP_PORT"),
            "",
            "no catcher when SMTP_HOST is set"
        );
    }

    #[test]
    fn a_dev_value_for_a_secret_is_refused() {
        let p = Project::new("secret", "[dev.env]\nAPP_TOKEN = \"committed\"\n");
        let problem = prepared(&p, &[field("APP_TOKEN", true)])
            .err()
            .expect("refused");
        assert!(problem.contains("`[dev.env] APP_TOKEN`"), "{problem}");
        assert!(problem.contains("[dev.secrets]"), "{problem}");
    }

    #[test]
    fn links_are_found_across_quoted_printable_line_breaks() {
        let data = "Subject: Paid\r\n\r\nSee http://localhost:8081/orders/=\r\nabc?x=3D1.\r\n<a href=\"http://localhost:8081/orders/abc?x=1\">x</a> https://k.example";
        assert_eq!(
            links(data),
            vec!["http://localhost:8081/orders/abc?x=1", "https://k.example"]
        );
    }
}

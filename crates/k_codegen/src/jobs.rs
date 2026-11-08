//! Background jobs (`docs/design/30-background-jobs.md`,
//! `docs/compiler/codegen.md` §15.4): every service implementing
//! `k.server.Job<Payload>` gets
//!
//! * a generated `static kind` (`OrderConfirmationJob.kind = { name }`),
//!   the typed handle `JobQueue.enqueue` takes;
//! * an entry in the server spec's `jobs` table, keyed by its name, whose
//!   `run(c, $c, json)` decodes the payload and calls `run(payload:)` on the
//!   service from container `c`;
//! * a [`JobSpec`] in the build manifest (its name, `@Retry` policy and
//!   `@Schedule`), which the server's jobs worker reads.
//!
//! A job's **name** is its `@JobName("…")`; else the name `storage.lock`
//! records for its declaration (`[jobs.<name>] declaration = "…"`), so a
//! renamed service keeps its queued rows; else its service name in kebab
//! case (`OrderConfirmationJob` → `order-confirmation-job`), which the
//! lockfile then records.

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, Type};
use k_syntax::ast::{self, AstNode};

use k_db::stdlib::{DEFAULT_ATTEMPTS, DEFAULT_BACKOFF_MS, JobSpec, Schedule};

use crate::cx::Gen;
use crate::js::{self, Expr, Param, Prop, Stmt, call, id, member};
use crate::lower::Body;

/// One job service of the program.
pub(crate) struct Job {
    /// The job's service declaration.
    pub decl: DeclId,
    /// Its name, policy and schedule.
    pub spec: JobSpec,
    /// The payload type (`None` for `Void`).
    pub payload: Option<Type>,
}

/// The program's job services, sorted by name.
pub(crate) fn jobs(g: &Gen<'_>) -> Vec<Job> {
    let Some(job) = g.view.lookup("k.server.Job") else {
        return Vec::new();
    };
    let locked = locked_names(g);
    let mut out: Vec<Job> = g
        .own
        .iter()
        .copied()
        .filter(|&d| {
            let info = g.view.decl(d);
            !info.is_test
                && info.named_kind() == Some(DeclarationKind::Service)
                && g.implements(d, job)
        })
        .map(|d| {
            let info = g.view.decl(d);
            let qualified = g.view.qualified_name(d);
            let node = g.decl_node(d);
            let annotation = |name: &str| -> Option<ast::Annotation> {
                node.as_ref()?
                    .children()
                    .filter_map(ast::Annotation::cast)
                    .find(|a| a.name().is_some_and(|t| t.text() == name))
            };
            let name = annotation("JobName")
                .and_then(|a| a.arg_list()?.args().next()?.value())
                .and_then(|v| string_literal(&v.syntax().text().to_string()))
                .or_else(|| {
                    locked
                        .iter()
                        .find(|(_, decl)| *decl == qualified)
                        .map(|(n, _)| n.clone())
                })
                .unwrap_or_else(|| crate::css::kebab(&info.name));
            let (mut attempts, mut backoff_ms) = (DEFAULT_ATTEMPTS, DEFAULT_BACKOFF_MS);
            if let Some(list) = annotation("Retry").and_then(|a| a.arg_list()) {
                for arg in list.args() {
                    let label = arg.label().map(|l| l.text().to_owned());
                    let text = arg
                        .value()
                        .map(|v| v.syntax().text().to_string())
                        .unwrap_or_default();
                    match label.as_deref() {
                        Some("attempts") => {
                            attempts = text.trim().parse().unwrap_or(attempts).max(1);
                        }
                        Some("backoff") => {
                            backoff_ms = duration_ms(&text).unwrap_or(backoff_ms);
                        }
                        _ => {}
                    }
                }
            }
            let schedule = annotation("Schedule")
                .and_then(|a| a.arg_list()?.args().next()?.value())
                .and_then(|v| schedule_of(&v.syntax().text().to_string()));
            let payload = info
                .implements
                .iter()
                .find(|t| t.decl() == Some(job))
                .and_then(|t| t.args().first().cloned())
                .filter(|t| !g.is(t, g.b.void) && !matches!(t, Type::Tuple(v) if v.is_empty()));
            Job {
                decl: d,
                spec: JobSpec {
                    name,
                    declaration: qualified,
                    attempts,
                    backoff_ms,
                    schedule,
                },
                payload,
            }
        })
        .collect();
    out.sort_by(|a, b| a.spec.name.cmp(&b.spec.name));
    out
}

/// `[jobs.<name>]` tables of `storage.lock` with their `declaration`.
fn locked_names(g: &Gen<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (file, path) in g.analysis.files() {
        if path.as_str() != "storage.lock" {
            continue;
        }
        let Some(text) = g.analysis.text(file) else {
            continue;
        };
        let mut current: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line
                .strip_prefix("[jobs.")
                .and_then(|r| r.strip_suffix(']'))
            {
                current = Some(rest.trim_matches('"').to_owned());
                continue;
            }
            if line.starts_with('[') {
                current = None;
                continue;
            }
            if let (Some(name), Some(value)) = (
                current.as_ref(),
                line.strip_prefix("declaration").map(str::trim),
            ) {
                if let Some(decl) = value
                    .strip_prefix('=')
                    .and_then(|v| string_literal(v.trim()))
                {
                    out.push((name.clone(), decl));
                }
            }
        }
    }
    out
}

/// The text of a plain string literal (`"order-confirmation"`).
fn string_literal(text: &str) -> Option<String> {
    let t = text.trim();
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains(['"', '\\', '{'])).then(|| inner.to_owned())
}

/// Milliseconds of a duration literal: `30.seconds`, `1.5.hours`,
/// `500.milliseconds`.
pub(crate) fn duration_ms(text: &str) -> Option<u64> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let (number, unit) = t.rsplit_once('.')?;
    let n: f64 = number.parse().ok()?;
    let scale = match unit {
        "millisecond" | "milliseconds" => 1.0,
        "second" | "seconds" => 1000.0,
        "minute" | "minutes" => 60_000.0,
        "hour" | "hours" => 3_600_000.0,
        "day" | "days" => 86_400_000.0,
        _ => return None,
    };
    let ms = n * scale;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (0.0..1e15).contains(&ms).then(|| ms.round() as u64)
}

/// Minutes after midnight of `TimeOfDay("03:00")`, read as the checked
/// literal it is (`k_sema::literals::time_of_day`, K2404).
fn time_of_day(text: &str) -> Option<u32> {
    let inner = text.trim().strip_prefix("TimeOfDay(")?.strip_suffix(')')?;
    let lit = string_literal(inner)?;
    let (h, m) = k_sema::literals::time_of_day(&lit).ok()?;
    Some(h * 60 + m)
}

/// A `ScheduleTime` value: `.every(1.hour)`, `.daily(at: TimeOfDay("03:00"))`,
/// `.weekly(on: .monday, at: TimeOfDay("03:00"))`.
pub(crate) fn schedule_of(text: &str) -> Option<Schedule> {
    let t = text.trim();
    let args = |prefix: &str| t.strip_prefix(prefix)?.strip_suffix(')');
    if let Some(a) = args(".every(") {
        return duration_ms(a).map(|interval_ms| Schedule::Every {
            interval_ms: interval_ms.max(1000),
        });
    }
    if let Some(a) = args(".daily(") {
        let at = a.trim().strip_prefix("at:")?;
        return time_of_day(at).map(|minute| Schedule::Daily { minute });
    }
    if let Some(a) = args(".weekly(") {
        let (on, at) = a.split_once(',')?;
        let day = on.trim().strip_prefix("on:")?.trim().strip_prefix('.')?;
        let weekday = [
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ]
        .iter()
        .position(|d| *d == day)?;
        let minute = time_of_day(at.trim().strip_prefix("at:")?)?;
        return Some(Schedule::Weekly {
            weekday: u32::try_from(weekday).ok()?,
            minute,
        });
    }
    None
}

/// `Job.kind = Object.freeze({ name: "…" })` for job service `d`.
pub(crate) fn kind_stmt(g: &Gen<'_>, d: DeclId) -> Option<Stmt> {
    let job = jobs(g).into_iter().find(|j| j.decl == d)?;
    Some(Stmt::Expr(js::assign(
        member(id(g.name(d)), "kind"),
        call(
            member(id("Object"), "freeze"),
            vec![Expr::Object(
                vec![Prop::KeyValue("name".into(), js::str(job.spec.name))],
                false,
            )],
        ),
    )))
}

/// The server spec's `jobs` table:
/// `{ "order-confirmation": { run: (c, $c, j) => c.X.run($c, codec.decode(j, "payload")) } }`.
pub(crate) fn table(b: &mut Body<'_, '_>) -> Expr {
    let g = b.g;
    let mut props = Vec::new();
    for job in jobs(g) {
        let service = member(id("c"), g.name(job.decl));
        let mut args = vec![id("$c")];
        if let Some(t) = &job.payload {
            let codec = crate::codec::codec_expr(b, t);
            args.push(call(
                member(codec, "decode"),
                vec![id("j"), js::str("payload")],
            ));
        }
        let run = js::arrow(
            vec![Param::new("c"), Param::new("$c"), Param::new("j")],
            call(member(service, "run"), args),
        );
        props.push(Prop::KeyValue(
            job.spec.name.clone(),
            Expr::Object(vec![Prop::KeyValue("run".into(), run)], false),
        ));
    }
    Expr::Object(props, true)
}

/// The manifest's job specs.
pub(crate) fn specs(g: &Gen<'_>) -> Vec<JobSpec> {
    jobs(g).into_iter().map(|j| j.spec).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_schedules_and_times_parse() {
        assert_eq!(duration_ms("30.seconds"), Some(30_000));
        assert_eq!(duration_ms("1.5.hours"), Some(5_400_000));
        assert_eq!(duration_ms("500.milliseconds"), Some(500));
        assert_eq!(duration_ms("2.fortnights"), None);
        assert_eq!(
            schedule_of(".every(1.hour)"),
            Some(Schedule::Every {
                interval_ms: 3_600_000
            })
        );
        assert_eq!(
            schedule_of(".daily(at: TimeOfDay(\"03:00\"))"),
            Some(Schedule::Daily { minute: 180 })
        );
        assert_eq!(
            schedule_of(".weekly(on: .friday, at: TimeOfDay(\"23:30\"))"),
            Some(Schedule::Weekly {
                weekday: 4,
                minute: 23 * 60 + 30
            })
        );
        assert_eq!(schedule_of(".daily(at: TimeOfDay(\"25:00\"))"), None);
        assert_eq!(string_literal("\"a-b\""), Some("a-b".into()));
        assert_eq!(string_literal("\"{x}\""), None);
    }
}

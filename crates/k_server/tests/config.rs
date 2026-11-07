//! Configuration from the environment: sources, precedence, parsing,
//! all-at-once reporting and secret redaction (`docs/design/25-config-and-secrets.md`).

use std::collections::HashMap;

use k_server::config::{
    ConfigField, ConfigKind, ConfigValue, Environment, Problem, report, resolve,
};

struct Env {
    vars: HashMap<String, String>,
    files: HashMap<String, String>,
}

impl Env {
    fn new(vars: &[(&str, &str)], files: &[(&str, &str)]) -> Env {
        Env {
            vars: vars
                .iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect(),
            files: files
                .iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect(),
        }
    }
}

impl Environment for Env {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| "No such file or directory".to_owned())
    }
}

fn field(env: &str, kind: ConfigKind) -> ConfigField {
    ConfigField {
        env: env.into(),
        declared_at: format!("app.Config.{}", env.to_lowercase()),
        kind,
        secret: false,
        optional: false,
        default: None,
    }
}

fn secret(env: &str) -> ConfigField {
    ConfigField {
        secret: true,
        ..field(env, ConfigKind::String)
    }
}

#[test]
fn values_come_from_the_variable_then_the_default() {
    let fields = [
        field("PORT", ConfigKind::Int),
        ConfigField {
            default: Some("25s".into()),
            ..field("K_SHUTDOWN_GRACE", ConfigKind::Duration)
        },
    ];
    let v = resolve(&fields, &Env::new(&[("PORT", "9000")], &[])).expect("valid");
    assert_eq!(v["PORT"], ConfigValue::Int(9000));
    assert_eq!(v["K_SHUTDOWN_GRACE"], ConfigValue::Duration(25_000.0));
}

#[test]
fn a_secret_file_wins_and_loses_one_trailing_line_ending() {
    let fields = [secret("STRIPE_KEY"), secret("CRLF"), secret("TWO")];
    let env = Env::new(
        &[
            ("STRIPE_KEY_FILE", "/run/secrets/stripe"),
            ("CRLF_FILE", "/c"),
            ("TWO_FILE", "/t"),
        ],
        &[
            ("/run/secrets/stripe", "sk_live_123\n"),
            ("/c", "abc\r\n"),
            ("/t", "x\n\n"),
        ],
    );
    let v = resolve(&fields, &env).expect("valid");
    assert_eq!(
        v["STRIPE_KEY"],
        ConfigValue::Secret(Box::new(ConfigValue::String("sk_live_123".into())))
    );
    assert_eq!(
        v["CRLF"],
        ConfigValue::Secret(Box::new(ConfigValue::String("abc".into())))
    );
    assert_eq!(
        v["TWO"],
        ConfigValue::Secret(Box::new(ConfigValue::String("x\n".into())))
    );
}

#[test]
fn every_problem_is_reported_at_once_sorted_by_variable() {
    let fields = [
        field("Z_REQUIRED", ConfigKind::String),
        field("A_PORT", ConfigKind::Int),
        field("M_FLAG", ConfigKind::Bool),
    ];
    let err = resolve(
        &fields,
        &Env::new(&[("A_PORT", "eighty"), ("M_FLAG", "yes")], &[]),
    )
    .expect_err("invalid");
    let vars: Vec<&str> = err.iter().map(|p| p.env.as_str()).collect();
    assert_eq!(vars, ["A_PORT", "M_FLAG", "Z_REQUIRED"]);
    assert!(err[2].message.contains("missing"));
}

#[test]
fn both_sources_set_is_a_problem_and_file_for_non_secret_too() {
    let fields = [secret("S"), field("P", ConfigKind::String)];
    let env = Env::new(
        &[("S", "a"), ("S_FILE", "/s"), ("P_FILE", "/p")],
        &[("/s", "b"), ("/p", "c")],
    );
    let err = resolve(&fields, &env).expect_err("invalid");
    assert_eq!(err.len(), 2);
    assert!(err[0].message.contains("isn't a secret"), "{:?}", err[0]);
    assert!(err[1].message.contains("both"), "{:?}", err[1]);
}

#[test]
fn an_unreadable_secret_file_is_a_problem() {
    let err = resolve(&[secret("S")], &Env::new(&[("S_FILE", "/nope")], &[])).expect_err("invalid");
    assert!(err[0].message.contains("can't be read"));
}

#[test]
fn secret_values_never_appear_in_problems_reports_or_debug_output() {
    let fields = [ConfigField {
        secret: true,
        ..field("TOKEN", ConfigKind::Int)
    }];
    let err = resolve(&fields, &Env::new(&[("TOKEN", "hunter2")], &[])).expect_err("not an Int");
    let text = report(&err);
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("Secret(•••)"), "{text}");
    let ok = resolve(&[secret("TOKEN")], &Env::new(&[("TOKEN", "hunter2")], &[])).expect("valid");
    assert!(!format!("{ok:?}").contains("hunter2"));
}

#[test]
fn optional_fields_default_to_nil() {
    let v = resolve(
        &[ConfigField {
            optional: true,
            ..secret("DATABASE_PASSWORD")
        }],
        &Env::new(&[], &[]),
    )
    .expect("valid");
    assert_eq!(v["DATABASE_PASSWORD"], ConfigValue::Nil);
}

#[test]
fn value_grammars() {
    let ok = |kind: ConfigKind, text: &str| {
        resolve(&[field("X", kind)], &Env::new(&[("X", text)], &[])).map(|v| v["X"].clone())
    };
    assert_eq!(
        ok(ConfigKind::Duration, "300ms"),
        Ok(ConfigValue::Duration(300.0))
    );
    assert_eq!(
        ok(ConfigKind::Duration, "10.5s"),
        Ok(ConfigValue::Duration(10_500.0))
    );
    assert_eq!(
        ok(ConfigKind::Duration, "2h"),
        Ok(ConfigValue::Duration(7_200_000.0))
    );
    assert_eq!(
        ok(ConfigKind::Duration, "1d"),
        Ok(ConfigValue::Duration(86_400_000.0))
    );
    for bad in ["5 minutes", "s", ".5s", "5.s", "-1s", "1.2.3s", ""] {
        assert!(ok(ConfigKind::Duration, bad).is_err(), "{bad:?}");
    }
    assert_eq!(ok(ConfigKind::Int, "-12"), Ok(ConfigValue::Int(-12)));
    assert!(
        ok(ConfigKind::Int, "9007199254740992").is_err(),
        "outside the safe range"
    );
    assert!(ok(ConfigKind::Int, "1.0").is_err());
    assert!(ok(ConfigKind::Double, "NaN").is_err());
    assert!(ok(ConfigKind::Bool, "True").is_err());
    let cases = ConfigKind::Enum(vec!["startTls".into(), "tls".into()]);
    assert_eq!(
        ok(cases.clone(), "tls"),
        Ok(ConfigValue::Enum("tls".into()))
    );
    assert!(ok(cases, "TLS").is_err());
    let list = ConfigKind::List(Box::new(ConfigKind::String));
    assert_eq!(
        ok(list.clone(), "10.0.0.0/8, 192.168.0.0/16"),
        Ok(ConfigValue::List(vec![
            ConfigValue::String("10.0.0.0/8".into()),
            ConfigValue::String("192.168.0.0/16".into())
        ]))
    );
    assert_eq!(ok(list, ""), Ok(ConfigValue::List(vec![])));
}

#[test]
fn the_report_matches_the_documented_layout() {
    let problems = vec![
        Problem {
            env: "STRIPE_WEBHOOK_SECRET".into(),
            message: "missing".into(),
        },
        Problem {
            env: "PORT".into(),
            message: "\"x\" is not an Int".into(),
        },
    ];
    assert_eq!(
        report(&problems),
        "error: configuration is invalid (2 problems)\n  STRIPE_WEBHOOK_SECRET  missing\n  PORT                   \"x\" is not an Int\n"
    );
}

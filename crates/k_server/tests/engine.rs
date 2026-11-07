//! The JavaScript engine: module loading, results, output capture,
//! uncaught errors, timeouts and isolation between engines.

use std::time::Duration;

use k_server::{Engine, EngineError, EngineOptions, JsModule, LogLine};

fn engine(modules: Vec<JsModule>) -> Engine {
    Engine::new(EngineOptions {
        modules,
        timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    })
    .expect("engine")
}

#[test]
fn a_module_reports_its_result() {
    let mut e = engine(vec![]);
    let r = e
        .run_main("Deno.core.ops.op_k_set_result(String(6 * 7));")
        .expect("runs");
    assert_eq!(r.as_deref(), Some("42"));
}

#[test]
fn a_module_without_a_result_returns_none() {
    let mut e = engine(vec![]);
    assert_eq!(e.run_main("const x = 1;").expect("runs"), None);
}

#[test]
fn top_level_await_and_microtasks_finish_before_returning() {
    let mut e = engine(vec![]);
    let r = e
        .run_main("let n = 0; queueMicrotask(() => n++); await null; await Promise.resolve(); Deno.core.ops.op_k_set_result(String(n));")
        .expect("runs");
    assert_eq!(r.as_deref(), Some("1"));
}

#[test]
fn the_runtime_and_app_modules_import() {
    let app = JsModule {
        specifier: "k:/app/shop/cart.js".into(),
        source: "import { signal } from \"k:runtime\";\nimport { twice } from \"./util.js\";\nexport const total = signal(twice(21));\n".into(),
    };
    let util = JsModule {
        specifier: "k:/app/shop/util.js".into(),
        source: "export const twice = (n) => n * 2;\n".into(),
    };
    let mut e = engine(vec![app, util]);
    let r = e
        .run_main("import { total } from \"k:/app/shop/cart.js\"; Deno.core.ops.op_k_set_result(String(total.get()));")
        .expect("runs");
    assert_eq!(r.as_deref(), Some("42"));
}

#[test]
fn console_output_is_captured_by_level() {
    let mut e = engine(vec![]);
    e.run_main(
        "console.log('a', 1, {b: 2}); console.warn('w'); console.error('e'); console.debug('d');",
    )
    .expect("runs");
    assert_eq!(
        e.take_output(),
        vec![
            LogLine {
                level: "info".into(),
                message: "a 1 {\"b\":2}".into()
            },
            LogLine {
                level: "warning".into(),
                message: "w".into()
            },
            LogLine {
                level: "error".into(),
                message: "e".into()
            },
            LogLine {
                level: "debug".into(),
                message: "d".into()
            },
        ]
    );
    assert!(e.take_output().is_empty(), "take_output clears");
}

#[test]
fn an_uncaught_exception_is_an_error_with_its_message() {
    let mut e = engine(vec![]);
    match e.run_main("throw new Error('boom');") {
        Err(EngineError::Uncaught(m)) => assert!(m.contains("boom"), "{m}"),
        other => panic!("expected Uncaught, got {other:?}"),
    }
}

#[test]
fn a_rejected_top_level_await_is_an_error() {
    let mut e = engine(vec![]);
    assert!(
        matches!(e.run_main("await Promise.reject(new Error('late'));"), Err(EngineError::Uncaught(m)) if m.contains("late"))
    );
}

#[test]
fn an_unknown_import_is_a_load_error() {
    let mut e = engine(vec![]);
    assert!(matches!(
        e.run_main("import \"k:/app/missing.js\";"),
        Err(EngineError::Load(_))
    ));
    let mut e = engine(vec![]);
    assert!(matches!(
        e.run_main("import \"https://example.com/x.js\";"),
        Err(EngineError::Load(_))
    ));
    let mut e = engine(vec![]);
    assert!(matches!(
        e.run_main("import \"node:fs\";"),
        Err(EngineError::Load(_))
    ));
}

#[test]
fn a_syntax_error_is_a_load_error() {
    let mut e = engine(vec![]);
    assert!(matches!(e.run_main("let = ;"), Err(EngineError::Load(_))));
}

#[test]
fn an_infinite_loop_is_terminated() {
    let mut e = Engine::new(EngineOptions {
        timeout: Some(Duration::from_millis(200)),
        ..Default::default()
    })
    .expect("engine");
    assert!(matches!(
        e.run_main("for (;;) {}"),
        Err(EngineError::Timeout(_))
    ));
}

#[test]
fn an_engine_runs_several_modules_in_sequence() {
    let mut e = engine(vec![]);
    e.run_main("globalThis.counter = 1;").expect("first");
    let r = e
        .run_main("Deno.core.ops.op_k_set_result(String(++globalThis.counter));")
        .expect("second");
    assert_eq!(r.as_deref(), Some("2"));
}

#[test]
fn engines_are_isolated() {
    // One engine per thread at a time (see `Engine`), as server workers do.
    let mut a = engine(vec![]);
    a.run_main("globalThis.secret = 'a';").expect("a");
    let r = std::thread::spawn(|| {
        let mut b = engine(vec![]);
        b.run_main("Deno.core.ops.op_k_set_result(typeof globalThis.secret);")
            .expect("b")
    })
    .join()
    .expect("thread");
    assert_eq!(r.as_deref(), Some("undefined"));
}

#[test]
fn no_ambient_io_is_exposed() {
    let mut e = engine(vec![]);
    let r = e
        .run_main("Deno.core.ops.op_k_set_result([typeof fetch, typeof require, typeof process, typeof Deno.readFile].join(','));")
        .expect("runs");
    assert_eq!(
        r.as_deref(),
        Some("undefined,undefined,undefined,undefined")
    );
}

/// A test engine (`k test`): `op_k_test_reset` gives every test a fresh
/// process (sessions, rate limits, shared state) and a fresh in-memory
/// database with the build's migrations applied.
#[test]
fn a_test_reset_gives_a_fresh_database_and_fresh_sessions() {
    use k_db::manifest::DbManifest;
    use k_db::migrate::Migration;
    let manifest = DbManifest {
        migrations: vec![Migration {
            name: "0001_notes".into(),
            sqlite: vec!["CREATE TABLE notes (title TEXT NOT NULL)".into()],
            postgres: vec!["CREATE TABLE notes (title TEXT NOT NULL)".into()],
        }],
        ..DbManifest::default()
    };
    let mut e = engine(vec![]);
    e.set_test_database(Some(manifest));
    let r = e
        .run_main(
            "const ops = Deno.core.ops;\n\
             ops.op_k_test_reset();\n\
             const id = await ops.op_k_session_create(\"ada\", []);\n\
             const before = (await ops.op_k_session_get(id)) !== null;\n\
             await ops.op_k_db_ping(1000);\n\
             ops.op_k_test_reset();\n\
             const after = (await ops.op_k_session_get(id)) !== null;\n\
             const db = await ops.op_k_db_ping(1000);\n\
             ops.op_k_set_result(JSON.stringify([before, after, db]));\n",
        )
        .expect("runs");
    assert_eq!(r.as_deref(), Some("[true,false,true]"));
}

/// Without a test database, a reset still renews the process, and the
/// app has no database.
#[test]
fn a_test_reset_without_a_database_has_none() {
    let mut e = engine(vec![]);
    let r = e
        .run_main(
            "const ops = Deno.core.ops;\n\
             ops.op_k_test_reset();\n\
             ops.op_k_set_result(String(await ops.op_k_db_ping(1000)));\n",
        )
        .expect("runs");
    assert_eq!(r.as_deref(), Some("false"));
}

/// `op_k_test_tick` lets a completed op deliver its result between runs of
/// microtasks (the kit's idle rounds), which microtasks alone never do.
#[test]
fn a_test_tick_lets_completed_ops_deliver() {
    let mut e = engine(vec![]);
    let r = e
        .run_main(
            "const ops = Deno.core.ops;\n\
             let done = false;\n\
             ops.op_k_password_hash(\"correct horse\").then(() => { done = true; });\n\
             for (let i = 0; i < 1000; i++) await null;\n\
             const before = done;\n\
             for (let i = 0; i < 1000000 && !done; i++) await ops.op_k_test_tick();\n\
             ops.op_k_set_result(JSON.stringify([before, done]));\n",
        )
        .expect("runs");
    assert_eq!(r.as_deref(), Some("[false,true]"));
}

/// In a test, the rate limiter counts on the test's virtual clock (the
/// optional `nowMs`), so advancing the `TestClock` past the window allows
/// calls again; with and without a database.
#[test]
fn rate_limits_follow_the_virtual_clock_in_tests() {
    for database in [false, true] {
        let mut e = engine(vec![]);
        if database {
            e.set_test_database(Some(k_db::manifest::DbManifest::default()));
        }
        let r = e
            .run_main(
                "const ops = Deno.core.ops;\n\
                 ops.op_k_test_reset();\n\
                 const t0 = Date.UTC(2026, 0, 1, 12, 0, 0);\n\
                 const out = [];\n\
                 for (let i = 0; i < 3; i++) out.push(await ops.op_k_rate_limit(\"k\", 2, 60000, t0 + i));\n\
                 out.push(await ops.op_k_rate_limit(\"k\", 2, 60000, t0 + 120000));\n\
                 ops.op_k_set_result(JSON.stringify(out.map((w) => w > 0)));\n",
            )
            .expect("runs");
        assert_eq!(
            r.as_deref(),
            Some("[false,false,true,false]"),
            "database: {database}"
        );
    }
}

//! The shop's optional Stripe binding end to end (the `shop` member of the
//! `examples/shop` workspace, with Stripe enabled in `App.k` exactly as its
//! documentation says, built for `.production` like `k build`):
//! `PlaceOrder` reserves stock, stores the order and starts a Stripe
//! Checkout session through `StripeCheckout`,
//! the stdlib's `FetchHttpClient` and `op_k_fetch`, against a fake Stripe
//! API (`k_server::fakes::FakeHttp`). What the fake receives is the request
//! the real Stripe would: `POST /v1/checkout/sessions`, form-encoded, with
//! the secret key as a bearer token and the order id as the idempotency key.
//!
//! The default build pays through paybox instead; `k_cli`'s
//! `dist_browser` tests run it against a real paybox `dist/app`.
//!
//! `docs/compiler/testing.md` §8.3.

// Tests fail by panicking.
#![allow(clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, resolve};
use k_server::fakes::{FakeHttp, FakeRoute, FakeSmtp};
use k_server::testing::{HttpResponse, send};
use k_server::{JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions};

/// The `examples/shop` workspace.
fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/shop")
}

/// The shop member of the workspace.
fn shop_dir() -> PathBuf {
    workspace_dir().join("shop")
}

/// The `use` rules of `App.k` that the Stripe binding replaces, and what
/// its documentation says to put in their place.
const PAYBOX_RULES: &str = "    use PayboxCheckout for PaymentGateway in .production, .development\n    use StripeDisabled for StripeEvents\n";
const STRIPE_RULES: [&str; 4] = [
    "use StripeCheckout for PaymentGateway in .production",
    "use PayboxCheckout for PaymentGateway in .development",
    "use StripeWebhookVerifier for StripeEvents in .production",
    "use StripeDisabled for StripeEvents in .development, .test",
];

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    for e in std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
    {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "dist" && name != "target" {
                collect(&path, root, out);
            }
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        let (Ok(p), Ok(text)) = (ProjectPath::new(&relative), std::fs::read_to_string(&path))
        else {
            continue;
        };
        out.push((p, text));
    }
}

/// The shop member's analysis files (with `pay-contracts` mounted), with
/// Stripe enabled in `App.k` when `stripe` is set.
fn shop_files(stripe: bool) -> Vec<(ProjectPath, String)> {
    let dir = workspace_dir();
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    let ws =
        k_sema::Workspace::from_files(files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)));
    let member = ws.member("shop").expect("the shop member");
    let mut files = ws.analysis_files(member);
    if stripe {
        let app = files
            .iter_mut()
            .find(|(p, _)| p.as_str() == "App.k")
            .expect("App.k");
        // The documentation shows the rules that enable Stripe; the test
        // applies exactly those, so the two can't drift apart.
        for rule in STRIPE_RULES {
            assert!(
                app.1.contains(&format!("///   {rule}\n")),
                "App.k documents `{rule}`"
            );
        }
        assert!(
            app.1.contains(PAYBOX_RULES),
            "App.k binds paybox by default"
        );
        let replacement: String = STRIPE_RULES.iter().map(|r| format!("    {r}\n")).collect();
        app.1 = format!(
            "import payments.StripeCheckout\nimport payments.StripeWebhookVerifier\n{}",
            app.1.replace(PAYBOX_RULES, &replacement)
        );
    }
    files
}

/// The shop as `k build` compiles it: release profile, production
/// environment; with Stripe enabled when `stripe` is set.
fn build(stripe: bool) -> Output {
    let host = AnalysisHost::with_files(SemaOptions::new(), shop_files(stripe));
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        panic!("the shop must check cleanly: {}: {}", d.code, d.message);
    }
    generate(
        &analysis,
        &Options {
            profile: Profile::Release,
            environment: Environment::Production,
            entry: Entry::App,
        },
    )
    .expect("the shop generates for production")
}

/// The shop with the optional Stripe binding enabled.
fn build_production() -> Output {
    build(true)
}

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> TempDb {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("k_shop_prod_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDb(dir)
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.0.join("shop.db").display())
    }

    fn rows(&self, sql: &str) -> Vec<Vec<k_db::Value>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let db = k_db::Database::connect(&self.url(), &k_db::DatabaseOptions::default())
                .await
                .expect("connects");
            db.query(sql, &[])
                .await
                .expect("queries")
                .into_iter()
                .map(|r| r.values)
                .collect()
        })
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SESSION: &str = r#"{"id":"cs_test_a1b2","url":"https://checkout.stripe.com/c/pay/cs_test_a1b2","object":"checkout.session"}"#;

/// The production server with Stripe at `stripe`, SMTP at `smtp`.
fn start(output: &Output, db: &TempDb, stripe: &FakeHttp, smtp: &FakeSmtp) -> Server {
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().expect("an entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    assert!(
        output
            .manifest
            .config
            .iter()
            .any(|c| c.env == "STRIPE_API_KEY" && c.required && c.secret),
        "production requires the Stripe key"
    );
    let env: Vec<(&str, String)> = vec![
        ("DATABASE_URL", db.url()),
        ("K_PUBLIC_ORIGIN", "https://shop.test".into()),
        ("STRIPE_API_KEY", "sk_test_4eC39HqLyjWDarjtT1zdp7dc".into()),
        ("STRIPE_API_URL", stripe.origin()),
        ("STRIPE_API_TIMEOUT", "2s".into()),
        ("STRIPE_WEBHOOK_SECRET", "whsec_test_secret".into()),
        ("SHOP_ADMIN_EMAIL", "admin@shop.example".into()),
        // The card page and paybox's events stay part of the shop with
        // Stripe enabled, so paybox's address is still configured; nothing
        // in these tests calls it.
        ("K_SERVICE_TOKEN", "shop_service_secret".into()),
        ("PAYBOX_URL", "http://127.0.0.1:9".into()),
        ("PAYBOX_TOKEN", "paybox_service_secret".into()),
        (
            "SHOP_ADMIN_PASSWORD_HASH",
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaA".into(),
        ),
        ("SMTP_HOST", "127.0.0.1".into()),
        ("SMTP_PORT", smtp.port().to_string()),
        ("SMTP_SECURITY", "none".into()),
        ("MAIL_FROM", "K Shop <orders@shop.example>".into()),
    ];
    let dev = k_server::config::DevEnvironment {
        root: shop_dir(),
        dotenv: env.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        with_process: false,
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 2;
    o.public_origin = "https://shop.test".into();
    o.shutdown_grace = Duration::from_secs(5);
    o.log = if std::env::var_os("K_E2E_LOG").is_some() {
        LogOutput::JsonLines
    } else {
        LogOutput::Capture
    };
    o.config =
        resolve(&fields, &dev).unwrap_or_else(|p| panic!("{}", k_server::config::report(&p)));
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    o.forgery_exempt = output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    o.jobs = output.manifest.jobs.clone();
    o.job_poll = Duration::from_millis(20);
    let server = Server::start(o).expect("the production shop starts");
    // The demo catalog, once the migrations created the tables.
    let catalog =
        std::fs::read_to_string(shop_dir().join("scripts/demo-catalog.sql")).expect("catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let db = k_db::Database::connect(&db.url(), &k_db::DatabaseOptions::default())
            .await
            .expect("connects");
        db.execute(&catalog, &[]).await.expect("loads the catalog");
    });
    server
}

fn place_order(server: &Server, body: &str) -> HttpResponse {
    send(
        server.local_addr(),
        "POST",
        "/shop.store.StoreProtocol/PlaceOrder",
        &[
            ("content-type".into(), "application/json".into()),
            ("connect-protocol-version".into(), "1".into()),
        ],
        body.as_bytes(),
    )
    .expect("answers")
}

fn json(r: &HttpResponse) -> serde_json::Value {
    serde_json::from_slice(&r.body)
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&r.body)))
}

fn stock(db: &TempDb, slug: &str) -> i64 {
    match db
        .rows(&format!("SELECT stock FROM product WHERE slug = '{slug}'"))
        .first()
        .and_then(|r| r.first())
    {
        Some(k_db::Value::Int(n)) => *n,
        other => panic!("no stock for {slug}: {other:?}"),
    }
}

#[test]
fn the_default_production_build_needs_no_stripe_configuration() {
    let output = build(false);
    let env: Vec<&str> = output
        .manifest
        .config
        .iter()
        .map(|c| c.env.as_str())
        .collect();
    for stripe in ["STRIPE_API_KEY", "STRIPE_WEBHOOK_SECRET"] {
        assert!(!env.contains(&stripe), "{stripe} in {env:?}");
    }
    for paybox in [
        "PAYBOX_URL",
        "PAYBOX_TOKEN",
        "PAYBOX_CALLBACK_URL",
        "K_SERVICE_TOKEN",
    ] {
        assert!(
            output
                .manifest
                .config
                .iter()
                .any(|c| c.env == paybox && c.required),
            "{paybox} is required: {env:?}"
        );
    }
    // With Stripe enabled, the callback address isn't needed: only
    // `PayboxCheckout` injects it.
    let stripe = build(true);
    assert!(
        !stripe
            .manifest
            .config
            .iter()
            .any(|c| c.env == "PAYBOX_CALLBACK_URL"),
        "{:?}",
        stripe.manifest.config
    );
}

#[test]
fn place_order_starts_a_stripe_checkout_and_redirects_to_it() {
    let output = build_production();
    let db = TempDb::new();
    let stripe = FakeHttp::start(vec![FakeRoute::json(
        "POST",
        "/v1/checkout/sessions",
        &serde_json::from_str(SESSION).expect("json"),
    )])
    .expect("fake Stripe");
    let smtp = FakeSmtp::start().expect("fake SMTP");
    let server = start(&output, &db, &stripe, &smtp);
    let before = stock(&db, "k-mug");

    let r = place_order(
        &server,
        r#"{"request": {"productSlug": "k-mug", "quantity": 2, "email": "  Ana@Example.COM "}}"#,
    );
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    let placed = json(&r);
    assert_eq!(
        placed["paymentUrl"],
        "https://checkout.stripe.com/c/pay/cs_test_a1b2"
    );
    let order_id = placed["orderId"].as_str().expect("an order id").to_owned();
    assert_eq!(stock(&db, "k-mug"), before - 2, "the stock is reserved");

    // Exactly the request Stripe's API documents.
    let seen = stripe.requests();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let call = &seen[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.path, "/v1/checkout/sessions");
    assert_eq!(
        call.headers["authorization"],
        "Bearer sk_test_4eC39HqLyjWDarjtT1zdp7dc"
    );
    assert_eq!(
        call.headers["idempotency-key"],
        format!("checkout-{order_id}")
    );
    assert_eq!(
        call.headers["content-type"],
        "application/x-www-form-urlencoded"
    );
    let field = |name: &str| call.form_field(name).unwrap_or_else(|| panic!("no {name}"));
    assert_eq!(field("mode"), "payment");
    assert_eq!(field("client_reference_id"), order_id);
    assert_eq!(
        field("customer_email"),
        "Ana@example.com",
        "the parsed, normalized address"
    );
    assert_eq!(field("line_items[0][quantity]"), "2");
    assert_eq!(field("line_items[0][price_data][currency]"), "usd");
    assert_eq!(field("line_items[0][price_data][unit_amount]"), "2400");
    assert_eq!(
        field("line_items[0][price_data][product_data][name]"),
        "K Mug"
    );
    assert_eq!(
        field("success_url"),
        format!("https://shop.test/receipt/{order_id}")
    );
    assert_eq!(field("cancel_url"), "https://shop.test/products/k-mug");

    // The order is awaiting payment.
    let r = send(
        server.local_addr(),
        "POST",
        "/shop.store.StoreProtocol/Order",
        &[
            ("content-type".into(), "application/json".into()),
            ("connect-protocol-version".into(), "1".into()),
        ],
        serde_json::json!({ "id": order_id }).to_string().as_bytes(),
    )
    .expect("answers");
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    let order = json(&r);
    assert_eq!(order["status"], "awaitingPayment", "{order}");

    // Stripe reports the payment: the signed webhook marks the order paid
    // and enqueues the confirmation in the same transaction; the jobs
    // worker sends it over SMTP, once however often Stripe redelivers.
    let event = serde_json::json!({
        "id": "evt_1",
        "type": "checkout.session.completed",
        "data": { "object": {
            "id": "cs_test_a1b2",
            "client_reference_id": order_id,
            "payment_status": "paid",
            "amount_total": 4800,
            "currency": "usd"
        } }
    })
    .to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_secs();
    let sign = |secret: &[u8], at: u64| {
        let mac = k_server::ops::hmac(256, secret, format!("{at}.{event}").as_bytes());
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        format!("t={at},v1={hex}")
    };
    let webhook = |signature: Option<String>| {
        let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
        if let Some(sig) = signature {
            headers.push(("stripe-signature".to_owned(), sig));
        }
        send(
            server.local_addr(),
            "POST",
            "/webhooks/stripe",
            &headers,
            event.as_bytes(),
        )
        .expect("answers")
        .status
    };
    assert_eq!(webhook(None), 400, "an unsigned delivery is refused");
    assert_eq!(
        webhook(Some(sign(b"whsec_attacker", now))),
        400,
        "a forged signature"
    );
    assert_eq!(
        webhook(Some(sign(b"whsec_test_secret", now - 600))),
        400,
        "a stale signature"
    );
    assert_eq!(
        webhook(Some(sign(b"whsec_test_secret", now))),
        204,
        "a signed delivery"
    );
    assert_eq!(
        webhook(Some(sign(b"whsec_test_secret", now))),
        204,
        "a redelivery"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while smtp.mail().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let mail = smtp.mail();
    assert_eq!(mail.len(), 1, "exactly one confirmation: {mail:?}");
    assert_eq!(mail[0].from, "orders@shop.example");
    assert_eq!(mail[0].to, ["Ana@example.com"]);
    assert_eq!(mail[0].subject, "Your K Shop order is confirmed");
    assert!(
        mail[0].data.contains("2 × K Mug") || mail[0].data.contains("2 =C3=97 K Mug"),
        "{}",
        mail[0].data
    );
    assert!(
        mail[0]
            .data
            .contains(&format!("https://shop.test/receipt/{order_id}")),
        "{}",
        mail[0].data
    );
    assert!(
        db.rows("SELECT id FROM k_job").is_empty(),
        "the job is done"
    );
    server.shutdown().expect("stops");
}

#[test]
fn stripe_failures_cancel_the_order_and_release_the_stock() {
    let output = build_production();
    let db = TempDb::new();
    let stripe = FakeHttp::start(vec![FakeRoute::text(
        "POST",
        "/v1/checkout/sessions",
        503,
        "Service Unavailable",
    )])
    .expect("fake Stripe");
    let smtp = FakeSmtp::start().expect("fake SMTP");
    let server = start(&output, &db, &stripe, &smtp);
    let before = stock(&db, "k-mug");
    let order =
        r#"{"request": {"productSlug": "k-mug", "quantity": 1, "email": "ana@example.com"}}"#;

    // A 5xx: the customer may retry later.
    let r = place_order(&server, order);
    assert_eq!(r.status, 400, "{}", String::from_utf8_lossy(&r.body));
    let e = json(&r);
    assert_eq!(e["code"], "failed_precondition");
    assert_eq!(
        e["details"][0]["debug"]["case"], "paymentUnavailable",
        "{e}"
    );
    assert_eq!(stock(&db, "k-mug"), before, "the reservation is released");

    // A slow Stripe is unavailable too, within STRIPE_API_TIMEOUT (2 s).
    stripe.set_routes(vec![
        FakeRoute::json(
            "POST",
            "/v1/checkout/sessions",
            &serde_json::from_str(SESSION).expect("json"),
        )
        .delayed(Duration::from_secs(5)),
    ]);
    let started = std::time::Instant::now();
    let r = place_order(&server, order);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the timeout applies"
    );
    assert_eq!(
        json(&r)["details"][0]["debug"]["case"],
        "paymentUnavailable"
    );

    // A 4xx (a wrong key): the order is cancelled; nothing to retry.
    stripe.set_routes(vec![FakeRoute::text(
        "POST",
        "/v1/checkout/sessions",
        401,
        "Invalid API Key provided",
    )]);
    let r = place_order(&server, order);
    assert_eq!(r.status, 400);
    assert_eq!(stock(&db, "k-mug"), before);
    // The secret key never reaches a response.
    assert!(!String::from_utf8_lossy(&r.body).contains("sk_test"));

    // Invalid requests never reach Stripe.
    let calls = stripe.requests().len();
    let r = place_order(
        &server,
        r#"{"request": {"productSlug": "k-mug", "quantity": 11, "email": "ana@example.com"}}"#,
    );
    assert_eq!(r.status, 400);
    assert_eq!(json(&r)["code"], "invalid_argument");
    assert_eq!(stripe.requests().len(), calls);
    server.shutdown().expect("stops");
}

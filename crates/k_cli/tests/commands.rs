//! End-to-end tests of `k routes`, `k migrate`, `k test` and the stale-asset
//! cleanup of `k build --out`, against golden output in `tests/golden/`.
//!
//! Each test runs the real `k` binary in a temporary project (or on an
//! example under `examples/`) and compares what it prints, byte for byte,
//! with the golden file, so a change of format is a reviewed diff.

// The helpers below fail the calling test with a message.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `k` with `args` in `dir`.
fn k(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TEST_FILTER")
        .output()
        .expect("run k")
}

/// A directory removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    /// Creates an empty directory named after `tag` and the process.
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_cmd_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Writes `text` to `dir/relative`, creating directories.
fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The repository root.
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The golden file `tests/golden/<name>`.
fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Asserts `actual` equals the golden file `name`, showing both on failure.
fn assert_golden(actual: &str, name: &str) {
    let expected = golden(name);
    assert!(
        actual == expected,
        "output differs from tests/golden/{name}\n--- expected\n{expected}--- actual\n{actual}"
    );
}

/// Standard output as text.
fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Standard error as text.
fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const MANIFEST: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

// ------------------------------------------------------------------ routes

#[test]
fn routes_prints_the_shop_table_with_endpoints_methods_guards_and_params() {
    // `examples/shop` is a workspace; the shop is its `shop` member.
    let out = k(&["routes", "examples/shop/shop"], &repo());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_golden(&stdout(&out), "routes-shop.txt");
}

#[test]
fn routes_of_a_server_only_member_list_its_endpoints_alone() {
    // Paybox serves `pay.Payments` and has no pages: only its health check.
    let out = k(&["routes", "examples/shop/paybox"], &repo());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_golden(&stdout(&out), "routes-paybox.txt");
}

#[test]
fn routes_at_a_workspace_root_names_the_members_to_run_it_in() {
    let out = k(&["routes", "examples/shop"], &repo());
    // A usage error: the command needs one project.
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("is a workspace root"), "{err}");
    assert!(err.contains("examples/shop/paybox"), "{err}");
}

#[test]
fn routes_prints_query_parameters_with_their_defaults() {
    let out = k(&["routes"], &repo().join("examples/notes"));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_golden(&stdout(&out), "routes-notes.txt");
}

#[test]
fn routes_lists_redirects_from_routes_lock_and_every_endpoint_method() {
    let dir = TempDir::new("routes");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "App.k", "public app Demo {}\n");
    write(
        &dir.0,
        "src/site/Home.k",
        "public page Home at \"/\" {\n    view {\n        heading(level: 1) { \"Home\" }\n    }\n}\n",
    );
    write(
        &dir.0,
        "src/site/Product.k",
        "public page Product at \"/products/{slug}\" {\n    @Param let slug: String\n    @Param let tab: String = \"info\"\n\n    view {\n        heading(level: 1) { \"{slug}\" }\n    }\n}\n",
    );
    write(
        &dir.0,
        "src/hooks/Ping.k",
        "import k.auth.RateLimit\nimport k.http.HttpResponse\nimport k.http.IncomingRequest\n\npublic page Ping requires RateLimit(10.perMinute) at \"/webhooks/ping\" {\n    @Post\n    func receive(request: IncomingRequest) async -> HttpResponse {\n        return HttpResponse(status: .noContent)\n    }\n\n    @Put\n    func replace(request: IncomingRequest) async -> HttpResponse {\n        return HttpResponse(status: .noContent)\n    }\n}\n",
    );
    write(
        &dir.0,
        "routes.lock",
        "[redirects]\n\"/old-home\" = \"home\"\n",
    );
    let out = k(&["routes"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_golden(&stdout(&out), "routes-fixture.txt");
}

#[test]
fn routes_of_a_project_without_pages_prints_only_the_header() {
    let dir = TempDir::new("routes_empty");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "src/a/A.k", "let a = 1\n");
    let out = k(&["routes"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "METHOD  URL  PAGE  PARAMS  GUARDS\n");
}

#[test]
fn routes_refuses_a_project_with_errors_and_a_non_project() {
    let dir = TempDir::new("routes_errors");
    let none = k(&["routes"], &dir.0);
    assert_eq!(none.status.code(), Some(2));
    assert!(stderr(&none).contains("no k.toml"), "{}", stderr(&none));
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "src/a/A.k", "let a: Int = \"x\"\n");
    let broken = k(&["routes"], &dir.0);
    assert_eq!(broken.status.code(), Some(1));
    assert!(
        stderr(&broken).contains("1 error(s)"),
        "{}",
        stderr(&broken)
    );
    assert!(stdout(&broken).is_empty());
}

// ------------------------------------------------------------------- build

/// The files below `dir`, relative, sorted.
fn files_below(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    if dir.is_dir() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}

/// A page whose heading says `text`.
fn home(text: &str) -> String {
    format!(
        "public page Home at \"/\" {{\n    view {{\n        heading(level: 1) {{ \"{text}\" }}\n    }}\n}}\n"
    )
}

#[test]
fn build_out_removes_stale_hashed_assets_and_keeps_other_files() {
    let dir = TempDir::new("stale");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "App.k", "public app Demo {}\n");
    write(&dir.0, "src/site/Home.k", &home("First"));
    let first = k(&["build", "--out", "out"], &dir.0);
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let assets = dir.0.join("out/public/assets");
    let before = files_below(&assets);
    assert!(!before.is_empty());
    // Leftovers of older builds, and a file outside the assets.
    write(&assets, "pages/old.0123abcd.js", "old");
    write(&assets, "old.deadbeef.css", "old");
    write(&assets, "nested/gone/x.0123abcd.js.map", "old");
    write(&dir.0, "out/notes.txt", "kept");
    write(&dir.0, "src/site/Home.k", &home("Second"));
    let second = k(&["build", "--out", "out"], &dir.0);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    let after = files_below(&assets);
    for stale in [
        "pages/old.0123abcd.js",
        "old.deadbeef.css",
        "nested/gone/x.0123abcd.js.map",
    ] {
        assert!(
            !after.contains(&stale.to_owned()),
            "{stale} is removed: {after:?}"
        );
    }
    assert!(
        !assets.join("nested").exists(),
        "emptied directories are removed"
    );
    assert_eq!(
        after.len(),
        before.len(),
        "the changed page's old chunk is replaced, not kept: {before:?} -> {after:?}"
    );
    assert_ne!(after, before, "the changed page has a new hash");
    assert_eq!(
        fs::read_to_string(dir.0.join("out/notes.txt")).unwrap(),
        "kept"
    );
    let rebuilt = k(&["build", "--out", "out"], &dir.0);
    assert_eq!(rebuilt.status.code(), Some(0));
    assert_eq!(
        files_below(&assets),
        after,
        "an unchanged build removes nothing"
    );
}

// ----------------------------------------------------------------- migrate

/// Copies `examples/<name>` (sources, migrations and lockfiles) to `dir`.
fn copy_example(name: &str, dir: &Path) {
    fn copy(from: &Path, to: &Path) {
        for e in fs::read_dir(from).unwrap().flatten() {
            let p = e.path();
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with('.') || n == "dist" {
                continue;
            }
            if p.is_dir() {
                fs::create_dir_all(to.join(&n)).unwrap();
                copy(&p, &to.join(&n));
            } else {
                fs::copy(&p, to.join(&n)).unwrap();
            }
        }
    }
    copy(&repo().join("examples").join(name), dir);
}

/// The fixture catalog of the `migrate` tests: `Product` (every column
/// kind) and `Review` (a relation), in an app, so `k check` compares its
/// entities, migrations and `schema.lock`.
fn catalog(dir: &Path) {
    write(dir, "k.toml", MANIFEST);
    write(dir, "App.k", "public app Catalog {}\n");
    write(
        dir,
        "src/catalog/Product.k",
        "import k.crypto.Uuid\n\n/// A product of the catalog.\npublic entity Product {\n    @Id let id: Uuid\n\n    @Unique let slug: String\n\n    var name: String\n\n    var price: Money\n\n    var status: ProductStatus = .draft\n\n    var note: String?\n\n    @Index let createdAt: Date\n}\n",
    );
    write(
        dir,
        "src/catalog/ProductStatus.k",
        "/// Whether a product is for sale.\npublic enum ProductStatus {\n    case draft\n    case live\n}\n",
    );
    write(
        dir,
        "src/catalog/Review.k",
        "import k.crypto.Uuid\n\n/// A review of a product.\npublic entity Review {\n    @Id let id: Uuid\n\n    @Relation let product: Product\n\n    let stars: Int\n}\n",
    );
}

#[test]
fn migrate_new_writes_the_first_migration_and_the_lock() {
    let dir = TempDir::new("migrate_first");
    catalog(&dir.0);
    let out = k(&["migrate", "new", "create catalog"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "wrote migrations/0001_create_catalog.k\nwrote schema.lock\n"
    );
    let migration = fs::read_to_string(dir.0.join("migrations/0001_create_catalog.k")).unwrap();
    assert_golden(&migration, "migrate-create-catalog.k");
    let lock = fs::read_to_string(dir.0.join("schema.lock")).unwrap();
    assert_golden(&lock, "migrate-create-catalog.lock");
    // The migration replays to the lock and matches the entities.
    let check = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let again = k(&["migrate", "new", "nothing"], &dir.0);
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    assert_eq!(
        stdout(&again),
        "nothing to migrate: the entities match schema.lock\n"
    );
    assert!(!dir.0.join("migrations/0002_nothing.k").exists());
}

#[test]
fn migrate_new_adds_a_column_with_the_fields_default() {
    let dir = TempDir::new("migrate_pinning");
    copy_example("notes", &dir.0);
    // Back to the state before pinning was added.
    fs::remove_file(dir.0.join("migrations/0002_add_note_pinning.k")).unwrap();
    let lock_path = dir.0.join("schema.lock");
    let lock = fs::read_to_string(&lock_path).unwrap();
    let lock = lock
        .replace("    \"0002_add_note_pinning\",\n", "")
        .replace(
            "    { field = \"isPinned\", name = \"is_pinned\", type = \"boolean\", default = false },\n",
            "",
        );
    fs::write(&lock_path, lock).unwrap();
    let out = k(&["migrate", "new", "add note pinning"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "wrote migrations/0002_add_note_pinning.k\nwrote schema.lock\n"
    );
    let migration = fs::read_to_string(dir.0.join("migrations/0002_add_note_pinning.k")).unwrap();
    assert_golden(&migration, "migrate-add-note-pinning.k");
    let lock = k_db::lock::SchemaLock::parse(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(
        lock.migrations,
        ["0001_create_accounts_and_notes", "0002_add_note_pinning"]
    );
    let pinned = lock.tables["note"]
        .columns
        .iter()
        .find(|c| c.name == "is_pinned")
        .unwrap();
    assert_eq!(pinned.default.as_deref(), Some("false"));
    assert_eq!(pinned.field.as_deref(), Some("isPinned"));
    let check = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn migrate_new_adds_columns_and_tables_and_retires_removed_fields() {
    let dir = TempDir::new("migrate_evolve");
    catalog(&dir.0);
    assert_eq!(
        k(&["migrate", "new", "create catalog"], &dir.0)
            .status
            .code(),
        Some(0)
    );
    // Remove `note`, add `stock` (with a default) and `subtitle` (optional),
    // and a new entity.
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    let product = product.replace("    var note: String?\n\n", "").replace(
        "    @Index let createdAt: Date\n",
        "    @Index let createdAt: Date\n\n    var stock: Int = 0\n\n    var subtitle: String?\n",
    );
    write(&dir.0, "src/catalog/Product.k", &product);
    write(
        &dir.0,
        "src/catalog/Tag.k",
        "import k.crypto.Uuid\n\n/// A label for products.\npublic entity Tag {\n    @Id let id: Uuid\n\n    let label: String\n}\n",
    );
    let out = k(&["migrate", "new", "evolve catalog"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let migration = fs::read_to_string(dir.0.join("migrations/0002_evolve_catalog.k")).unwrap();
    assert_golden(&migration, "migrate-evolve.k");
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    let note = lock.tables["product"]
        .columns
        .iter()
        .find(|c| c.name == "note")
        .unwrap();
    assert!(note.retired, "{note:?}");
    assert_eq!(note.field, None);
    assert!(lock.tables.contains_key("tag"));
    let check = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn migrate_new_refuses_a_required_column_without_a_default() {
    let dir = TempDir::new("migrate_required");
    catalog(&dir.0);
    assert_eq!(
        k(&["migrate", "new", "create catalog"], &dir.0)
            .status
            .code(),
        Some(0)
    );
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Product.k",
        &product.replace(
            "    var name: String\n",
            "    var name: String\n\n    var sku: String\n",
        ),
    );
    let lock_before = fs::read_to_string(dir.0.join("schema.lock")).unwrap();
    let out = k(&["migrate", "new", "add sku"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains(
            "`catalog.Product.sku` is added to the existing table `product`, so it needs a default value or an optional type"
        ),
        "{}",
        stderr(&out)
    );
    assert!(!dir.0.join("migrations/0002_add_sku.k").exists());
    assert_eq!(
        fs::read_to_string(dir.0.join("schema.lock")).unwrap(),
        lock_before,
        "nothing is written"
    );
}

#[test]
fn migrate_new_refuses_changes_it_cannot_write() {
    let dir = TempDir::new("migrate_refuse");
    catalog(&dir.0);
    assert_eq!(
        k(&["migrate", "new", "create catalog"], &dir.0)
            .status
            .code(),
        Some(0)
    );
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Product.k",
        &product.replace("    var name: String\n", "    var name: Int\n"),
    );
    let out = k(&["migrate", "new", "change name"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`catalog.Product.name` changed from `text` to `integer`"),
        "{}",
        stderr(&out)
    );
    write(&dir.0, "src/catalog/Product.k", &product);
    write(
        &dir.0,
        "src/catalog/ProductStatus.k",
        "/// Whether a product is for sale.\npublic enum ProductStatus {\n    case draft\n    case live\n    case archived\n}\n",
    );
    let out = k(&["migrate", "new", "archive"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`catalog.Product.status` gained the case `archived`"),
        "{}",
        stderr(&out)
    );
    assert!(!dir.0.join("migrations/0002_archive.k").exists());
}

#[test]
fn migrate_new_empty_writes_an_empty_migration() {
    let dir = TempDir::new("migrate_empty");
    catalog(&dir.0);
    assert_eq!(
        k(&["migrate", "new", "create catalog"], &dir.0)
            .status
            .code(),
        Some(0)
    );
    let out = k(&["migrate", "new", "--empty", "Seed the plans"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(dir.0.join("migrations/0002_seed_the_plans.k")).unwrap();
    assert_eq!(
        text,
        "// Generated by `k migrate new --empty \"Seed the plans\"`: write its data\n// changes with the schema builder (19-persistence.md, \"Data migrations\").\n\n/// Seed the plans.\nmigration SeedThePlans {\n    func up(schema: Schema) {\n    }\n}\n"
    );
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    assert_eq!(
        lock.migrations,
        ["0001_create_catalog", "0002_seed_the_plans"]
    );
    let check = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

/// `k check` in `dir`, with its exit code and its plain diagnostics.
fn check(dir: &Path) -> (Option<i32>, String) {
    let out = k(&["check", "--color", "never"], dir);
    (out.status.code(), stderr(&out))
}

/// The catalog after its first migration, `0001_create_catalog`.
fn migrated_catalog(dir: &Path) {
    catalog(dir);
    let out = k(&["migrate", "new", "create catalog"], dir);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn check_reports_entities_that_differ_from_schema_lock() {
    let dir = TempDir::new("check_drift");
    migrated_catalog(&dir.0);
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Product.k",
        &product.replace(
            "    var note: String?\n",
            "    var note: String?\n\n    var featured: Bool = false\n",
        ),
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("error[K8004]: `catalog.Product.featured` has no column in schema.lock"),
        "{err}"
    );
    assert!(
        err.contains("help: run `k migrate new \"description\"`: it writes the migration and records it in schema.lock"),
        "{err}"
    );
    // The suggested command resolves it.
    let out = k(&["migrate", "new", "feature products"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn migrate_new_drop_retired_drops_what_earlier_migrations_retired() {
    let dir = TempDir::new("migrate_drop_retired");
    migrated_catalog(&dir.0);
    let nothing = k(
        &["migrate", "new", "--drop-retired", "drop nothing"],
        &dir.0,
    );
    assert_eq!(nothing.status.code(), Some(0), "{}", stderr(&nothing));
    assert_eq!(
        stdout(&nothing),
        "nothing to drop: schema.lock records no retired table or column\n"
    );
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Product.k",
        &product.replace("    var note: String?\n\n", ""),
    );
    let out = k(&["migrate", "new", "retire the note"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "the retirement replays to the lock: {err}");
    let out = k(
        &["migrate", "new", "--drop-retired", "drop the note column"],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "wrote migrations/0003_drop_the_note_column.k\nwrote schema.lock\n"
    );
    let migration =
        fs::read_to_string(dir.0.join("migrations/0003_drop_the_note_column.k")).unwrap();
    assert_golden(&migration, "migrate-drop-retired.k");
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    assert!(
        !lock.tables["product"]
            .columns
            .iter()
            .any(|c| c.name == "note"),
        "the dropped column leaves the lock"
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn a_drop_of_a_live_column_written_by_hand_fails_check() {
    let dir = TempDir::new("check_live_drop");
    migrated_catalog(&dir.0);
    let out = k(&["migrate", "new", "--empty", "drop the slug"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    write(
        &dir.0,
        "migrations/0002_drop_the_slug.k",
        "/// Drops the slug.\nmigration DropTheSlug {\n    func up(schema: Schema) {\n        schema.alterTable(name: \"product\") { table =>\n            table.dropColumn(name: \"slug\")\n        }\n    }\n}\n",
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("error[K6006]: `0002_drop_the_slug` drops the column `product.slug`, which the previous release still reads during a rolling deploy"),
        "{err}"
    );
    assert!(err.contains("k migrate new --drop-retired"), "{err}");
    // The build refuses it too.
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(2), "{}", stderr(&build));
    assert!(
        stderr(&build).contains("drops the column `product.slug`"),
        "{}",
        stderr(&build)
    );
}

#[test]
fn migrate_rename_keeps_the_column_and_migrate_new_rename_renames_it() {
    let dir = TempDir::new("migrate_rename_field");
    migrated_catalog(&dir.0);
    let product = fs::read_to_string(dir.0.join("src/catalog/Product.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Product.k",
        &product.replace("    var name: String\n", "    var title: String\n"),
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("keep its storage with `k migrate rename catalog.Product.name title`"),
        "{err}"
    );
    let out = k(
        &["migrate", "rename", "catalog.Product.name", "title"],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "recorded `catalog.Product.name` as `title`; its storage keeps its name, so no migration is needed\nwrote schema.lock\n"
    );
    assert!(!dir.0.join("migrations/0002_rename.k").exists());
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "the field reads its old column: {err}");
    // Renaming the column too is a physical, single-instance change.
    let out = k(
        &[
            "migrate",
            "new",
            "--rename",
            "catalog.Product.title",
            "rename the title column",
        ],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let migration =
        fs::read_to_string(dir.0.join("migrations/0002_rename_the_title_column.k")).unwrap();
    assert_golden(&migration, "migrate-rename-column.k");
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    let title = lock.tables["product"]
        .columns
        .iter()
        .find(|c| c.field.as_deref() == Some("title"))
        .unwrap();
    assert_eq!(title.name, "title");
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
    // Nothing is left to rename.
    let again = k(
        &[
            "migrate",
            "new",
            "--rename",
            "catalog.Product.title",
            "again",
        ],
        &dir.0,
    );
    assert_eq!(again.status.code(), Some(1));
    assert!(
        stderr(&again).contains("`catalog.Product.title` is already stored in column")
            || stderr(&again).contains("can't move to column `product.title`"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn migrate_rename_records_a_renamed_entity_and_its_table_can_follow() {
    let dir = TempDir::new("migrate_rename_entity");
    migrated_catalog(&dir.0);
    let review = fs::read_to_string(dir.0.join("src/catalog/Review.k")).unwrap();
    fs::remove_file(dir.0.join("src/catalog/Review.k")).unwrap();
    write(
        &dir.0,
        "src/catalog/Rating.k",
        &review
            .replace("public entity Review", "public entity Rating")
            .replace("A review of", "A rating of"),
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("`k migrate rename catalog.Review catalog.Rating`"),
        "{err}"
    );
    let unknown = k(
        &["migrate", "rename", "catalog.Revue", "catalog.Rating"],
        &dir.0,
    );
    assert_eq!(unknown.status.code(), Some(1));
    assert!(
        stderr(&unknown).contains("schema.lock records no entity `catalog.Revue`"),
        "{}",
        stderr(&unknown)
    );
    let out = k(
        &["migrate", "rename", "catalog.Review", "catalog.Rating"],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
    let out = k(
        &[
            "migrate",
            "new",
            "--rename",
            "catalog.Rating",
            "rename the ratings table",
        ],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let migration =
        fs::read_to_string(dir.0.join("migrations/0002_rename_the_ratings_table.k")).unwrap();
    assert_golden(&migration, "migrate-rename-table.k");
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    assert_eq!(lock.tables["rating"].entity, "catalog.Rating");
    assert!(!lock.tables.contains_key("review"));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn build_records_a_hand_written_migration_in_schema_lock() {
    let dir = TempDir::new("build_schema_lock");
    migrated_catalog(&dir.0);
    let out = k(&["migrate", "new", "--empty", "archive products"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    write(
        &dir.0,
        "migrations/0002_archive_products.k",
        "/// Products can be archived.\nmigration ArchiveProducts {\n    func up(schema: Schema) {\n        schema.alterTable(name: \"product\") { table =>\n            table.addEnumCase(column: \"status\", case: \"archived\")\n        }\n    }\n}\n",
    );
    write(
        &dir.0,
        "src/catalog/ProductStatus.k",
        "/// Whether a product is for sale.\npublic enum ProductStatus {\n    case draft\n    case live\n    case archived\n}\n",
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("error[K8005]"), "{err}");
    assert!(err.contains("run `k build`"), "{err}");
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(0), "{}", stderr(&build));
    assert!(
        stderr(&build).contains("wrote schema.lock"),
        "{}",
        stderr(&build)
    );
    let lock =
        k_db::lock::SchemaLock::parse(&fs::read_to_string(dir.0.join("schema.lock")).unwrap())
            .unwrap();
    let status = lock.tables["product"]
        .columns
        .iter()
        .find(|c| c.name == "status")
        .unwrap();
    assert_eq!(status.cases, ["draft", "live", "archived"]);
    assert_eq!(status.field.as_deref(), Some("status"));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(0), "{err}");
    // A second build has nothing to record.
    let again = k(&["build"], &dir.0);
    assert!(
        !stderr(&again).contains("wrote schema.lock"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn build_never_drops_a_listed_migration_whose_file_is_gone() {
    let dir = TempDir::new("build_lost_migration");
    migrated_catalog(&dir.0);
    let out = k(&["migrate", "new", "--empty", "seed the catalog"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let lock = fs::read_to_string(dir.0.join("schema.lock")).unwrap();
    fs::remove_file(dir.0.join("migrations/0002_seed_the_catalog.k")).unwrap();
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(0), "{}", stderr(&build));
    assert_eq!(
        fs::read_to_string(dir.0.join("schema.lock")).unwrap(),
        lock,
        "the lost migration stays listed"
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("schema.lock lists the migration `0002_seed_the_catalog`, but `migrations/0002_seed_the_catalog.k` doesn't exist"),
        "{err}"
    );
}

/// A site with a named home page, `Pricing`, `Product` and an endpoint.
fn site(dir: &Path) {
    write(dir, "k.toml", MANIFEST);
    write(
        dir,
        "App.k",
        "public app Site {\n    metadata {\n        title(\"Site\")\n    }\n}\n",
    );
    write(
        dir,
        "src/site/Home.k",
        "public page Home at \"/\" {\n    view {\n        heading(level: 1) { \"Home\" }\n    }\n}\n",
    );
    write(
        dir,
        "src/site/Pricing.k",
        "public page Pricing {\n    view {\n        heading(level: 1) { \"Pricing\" }\n    }\n}\n",
    );
    write(
        dir,
        "src/site/Product.k",
        "import k.crypto.Uuid\n\npublic page Product {\n    @Param let id: Uuid\n\n    view {\n        heading(level: 1) { \"Product\" }\n    }\n}\n",
    );
    write(
        dir,
        "src/site/Health.k",
        "import k.http.HttpResponse\nimport k.http.IncomingRequest\n\npublic page Health at \"/health\" {\n    @Get\n    func check(request: IncomingRequest) async -> HttpResponse {\n        return HttpResponse(status: .ok)\n    }\n}\n",
    );
}

#[test]
fn build_writes_routes_lock_and_the_redirect_of_a_renamed_page() {
    let dir = TempDir::new("build_routes_lock");
    site(&dir.0);
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(0), "{}", stderr(&build));
    assert!(
        stderr(&build).contains("wrote routes.lock"),
        "{}",
        stderr(&build)
    );
    let lock = fs::read_to_string(dir.0.join("routes.lock")).unwrap();
    assert!(
        lock.contains("[pages.pricing]\ndeclaration = \"site.Pricing\"\nurl = \"/pricing\"\n"),
        "{lock}"
    );
    assert_eq!(check(&dir.0).0, Some(0));
    // Rename `Pricing` to `Plans`: its URL moves to `/plans`.
    fs::remove_file(dir.0.join("src/site/Pricing.k")).unwrap();
    write(
        &dir.0,
        "src/site/Plans.k",
        "public page Plans {\n    view {\n        heading(level: 1) { \"Plans\" }\n    }\n}\n",
    );
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("error[K8007]: `site.Plans` was renamed from `site.Pricing`, so its URL moved from `/pricing` to `/plans`"),
        "{err}"
    );
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(0), "{}", stderr(&build));
    let lock = fs::read_to_string(dir.0.join("routes.lock")).unwrap();
    assert_golden(&lock, "routes-lock-renamed.lock");
    assert_eq!(check(&dir.0).0, Some(0));
    let routes = k(&["routes"], &dir.0);
    assert!(
        stdout(&routes).contains("GET     /pricing       Plans (redirect)"),
        "{}",
        stdout(&routes)
    );
    // Removing a page keeps its entry, so `k check` keeps failing until a
    // redirect is added or the entry is deleted.
    fs::remove_file(dir.0.join("src/site/Plans.k")).unwrap();
    let build = k(&["build"], &dir.0);
    assert_eq!(build.status.code(), Some(0), "{}", stderr(&build));
    let (code, err) = check(&dir.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("`/plans` (page `plans`) is no longer served"),
        "{err}"
    );
}

#[test]
fn migrate_status_and_apply_use_the_development_database() {
    let dir = TempDir::new("migrate_apply");
    copy_example("notes", &dir.0);
    let db = dir.0.join("dev.db");
    write(
        &dir.0,
        ".env",
        &format!("DATABASE_URL=sqlite:{}\n", db.display()),
    );
    let status = k(&["migrate", "status"], &dir.0);
    assert_eq!(status.status.code(), Some(0), "{}", stderr(&status));
    assert_eq!(
        stdout(&status),
        "pending  0001_create_accounts_and_notes\npending  0002_add_note_pinning\n"
    );
    let apply = k(&["migrate", "apply"], &dir.0);
    assert_eq!(apply.status.code(), Some(0), "{}", stderr(&apply));
    assert_eq!(
        stdout(&apply),
        "applied migration 0001_create_accounts_and_notes\napplied migration 0002_add_note_pinning\n"
    );
    let status = k(&["migrate", "status"], &dir.0);
    assert_eq!(
        stdout(&status),
        "applied  0001_create_accounts_and_notes\napplied  0002_add_note_pinning\n"
    );
    let again = k(&["migrate", "apply"], &dir.0);
    assert_eq!(stdout(&again), "the database is up to date\n");
}

/// Without `DATABASE_URL` (environment, `.env` or `[dev.env]`), `k migrate
/// apply` migrates the database `k dev` would open,
/// `sqlite:.k/<package>.db` (13-tooling.md).
#[test]
fn migrate_apply_without_a_database_url_uses_the_development_database() {
    let dir = TempDir::new("migrate_nourl");
    copy_example("notes", &dir.0);
    let out = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["migrate", "apply"])
        .current_dir(&dir.0)
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        dir.0.join(".k/notes.db").is_file(),
        "no development database"
    );
}

// -------------------------------------------------------------------- test

/// The `k test` fixture project.
fn counter() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/counter")
}

#[test]
fn test_runs_every_test_and_reports_failures_with_their_source() {
    let out = k(&["test", "--color", "never"], &counter());
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_golden(&stdout(&out), "test-counter.txt");
}

#[test]
fn test_filters_by_name() {
    let out = k(
        &[
            "test",
            "--color",
            "never",
            "--filter",
            "increments",
            "--filter",
            "clicking",
        ],
        &counter(),
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_golden(&stdout(&out), "test-counter-filtered.txt");
}

#[test]
fn test_with_a_filter_that_matches_nothing_exits_with_1() {
    let out = k(&["test", "--filter", "no such test"], &counter());
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("no test matches `no such test`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn test_of_a_project_without_tests_succeeds() {
    let dir = TempDir::new("test_none");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "src/a/A.k", "let a = 1\n");
    let out = k(&["test"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 filtered out\n"
    );
}

#[test]
fn test_refuses_a_project_with_errors_and_a_non_project() {
    let dir = TempDir::new("test_errors");
    assert_eq!(k(&["test"], &dir.0).status.code(), Some(2));
    write(&dir.0, "k.toml", MANIFEST);
    write(
        &dir.0,
        "src/a/A.test.k",
        "test A {\n    test \"x\" {\n        expect 1\n    }\n}\n",
    );
    let out = k(&["test", "--color", "never"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("error[K7501]"),
        "the errors are printed: {}",
        stderr(&out)
    );
}

// ------------------------------------------------------- check runs tests

/// `k check` runs the project's tests (13-tooling.md, "Commands"): each
/// failure is a `K7510` diagnostic like `k test`'s, and the summary counts
/// the tests that ran.
#[test]
fn check_runs_the_tests_and_reports_each_failure() {
    let out = k(&["check", "--color", "never"], &counter());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert_eq!(err.matches("error[K7510]").count(), 3, "{err}");
    assert!(
        err.contains("in test `CounterView › a missing element fails the test with the query`"),
        "{err}"
    );
    assert!(
        err.trim_end()
            .ends_with("checked 5 files, ran 9 tests: 3 error(s), 0 warning(s)"),
        "{err}"
    );
}

/// `--no-tests` skips them: the check is the semantic pass and the
/// lockfiles only.
#[test]
fn check_no_tests_skips_the_tests() {
    let out = k(&["check", "--color", "never", "--no-tests"], &counter());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stderr(&out), "checked 5 files: no problems\n");
}

/// In JSON Lines, a failed test is one more diagnostic.
#[test]
fn check_json_reports_failed_tests_as_diagnostics() {
    let out = k(&["check", "--format", "json"], &counter());
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    let failures = text
        .lines()
        .filter(|l| l.contains("\"code\":\"K7510\""))
        .count();
    assert_eq!(failures, 3, "{text}");
}

/// Tests run only for a program without errors, as in `k test`.
#[test]
fn check_runs_no_tests_for_a_project_with_errors() {
    let dir = TempDir::new("check_tests_errors");
    write(&dir.0, "k.toml", MANIFEST);
    write(
        &dir.0,
        "src/a/A.k",
        "func one() -> Int {\n    return \"x\"\n}\n",
    );
    write(
        &dir.0,
        "src/a/A.test.k",
        "test A {\n    test \"fails\" {\n        expect 1 == 2\n    }\n}\n",
    );
    let out = k(&["check", "--color", "never"], &dir.0);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("error[K2001]"), "{err}");
    assert!(!err.contains("K7510") && !err.contains("ran "), "{err}");
}

/// A passing suite: the summary says how many ran.
#[test]
fn check_passes_the_todos_example_with_its_tests() {
    let out = k(&["check", "--color", "never", "examples/todos"], &repo());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    let last = err.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("checked ") && last.contains(" tests: no problems"),
        "{err}"
    );
}

#[test]
fn test_runs_the_todos_example() {
    let out = k(&["test", "--color", "never", "examples/todos"], &repo());
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("test result: ok."), "{text}");
}

/// Runs `k test` on `examples/<name>` and asserts that every test ran and
/// passed: the report has no `FAIL` line and says `ok`.
fn assert_example_passes(name: &str) {
    let out = k(
        &["test", "--color", "never", &format!("examples/{name}")],
        &repo(),
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("test result: ok."), "{text}");
    assert!(text.contains("0 failed"), "{text}");
}

#[test]
fn test_runs_the_weather_example() {
    // Client services against a TestHttpClient double and a fake service,
    // pages rendered with their loads.
    assert_example_passes("weather");
}

#[test]
fn test_runs_the_notes_example() {
    // Integration tests through the in-process server: sessions, guards,
    // repositories on a fresh database, a server stream.
    assert_example_passes("notes");
}

#[test]
fn test_runs_the_kanban_example() {
    // Drag and drop with the pointer and the keyboard, keyboard shortcuts,
    // @Form card editing, import and export through TestFiles, local
    // storage through MemoryStorage, progress on the TestClock.
    assert_example_passes("kanban");
}

#[test]
fn check_and_build_the_kanban_example_to_static_files() {
    let check = k(&["check", "examples/kanban"], &repo());
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let dir = TempDir::new("kanban_build");
    let out = dir.0.join("dist");
    let out_arg = out.to_string_lossy().into_owned();
    let built = k(&["build", "examples/kanban", "--out", &out_arg], &repo());
    assert_eq!(built.status.code(), Some(0), "{}", stderr(&built));
    assert!(!out.join("app").exists(), "kanban is static-only");
    let assets = files_below(&out.join("public/assets"));
    for chunk in ["drag.", "files.", "keys.", "charts.", "forms."] {
        assert!(
            assets.iter().any(|a| a.starts_with(chunk)),
            "kanban ships the {chunk}… feature chunk: {assets:?}"
        );
    }
    assert!(
        !assets
            .iter()
            .any(|a| a.starts_with("streams.") || a.starts_with("money.")),
        "{assets:?}"
    );
}

#[test]
fn test_runs_the_shop_example() {
    // A workspace: every member's tests. Server-side `use` rules, configs
    // bound by value, InlineJobQueue, RecordingEmailProvider, TestClient
    // through the endpoint pages, and fakes of a library protocol.
    assert_example_passes("shop");
    let out = k(&["test", "--color", "never", "examples/shop"], &repo());
    let text = stdout(&out);
    for member in ["== pay-contracts", "== paybox", "== shop"] {
        assert!(text.contains(member), "{text}");
    }
}

/// The fixture of `k test` regressions: every test must pass.
fn regressions() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/regressions")
}

#[test]
fn test_passes_every_regression_of_the_runner() {
    let out = k(&["test", "--color", "never"], &regressions());
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
}

#[test]
fn test_copies_and_shares_through_the_clipboard_and_share_doubles() {
    // `Clipboard`, `Share`, `CopyButton` and `ShareButton` against
    // `TestClipboard` and `TestShare` (45-web-platform.md): the copy and
    // its status on the `TestClock`, failures with and without a listener,
    // the share sheet and the copy where the browser can't share.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sharing");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
    assert!(
        text.contains("PASS  ShareInvite › where the browser can't share, the button copies the link and says so"),
        "{text}"
    );
}

#[test]
fn test_locates_through_the_geolocation_double() {
    // `Geolocation` against `TestGeolocation` (45-web-platform.md): the
    // prompt from a click, reads that never prompt, a refused or blocked
    // permission, no fix, `watch` following moves and ending when the
    // permission is revoked, and `Position.sample` on the `TestClock`.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/locating");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
    assert!(
        text.contains("PASS  Follower › revoking the permission ends the watch, and the effect, run again, finds it denied"),
        "{text}"
    );
}

#[test]
fn test_follows_the_environment_doubles_and_element_events() {
    // `PageVisibility`, `Connectivity`, `Preferences` and `Viewport`
    // against their doubles, and `appeared`, `disappeared` and `resized`
    // driven from handles (45-web-platform.md): an effect that stops while
    // the page is hidden, an offline banner, each preference, a phone-sized
    // viewport with a finger, infinite scroll, and measurements passed to
    // sync and async handlers.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/environment");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
    for name in [
        "PASS  OfflineBanner › going offline shows the banner, and coming back hides it",
        "PASS  Refresher › it refreshes while the page is visible and pauses while hidden",
        "PASS  Motion › each preference follows the system",
        "PASS  Layout › a phone gets the sheet and taps",
        "PASS  Feed › appeared fires on each entry, disappeared on each exit",
        "PASS  Measured › an async handler gets the size too",
    ] {
        assert!(text.contains(name), "{name}\n{text}");
    }
}

#[test]
fn test_messages_tabs_and_keeps_the_screen_on_through_their_doubles() {
    // `TabMessaging` against `TestTabMessaging` and `KeepScreenAwake`
    // against `TestWakeLock` (45-web-platform.md): posts recorded and never
    // heard by their own tab, deliveries reaching a service's `listen` and an
    // effect's `messages`, the lock held only while the component is
    // mounted, and a refusal firing `failed`.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tabs");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
    for name in [
        "PASS  CartBadge › a change another tab posts updates this one",
        "PASS  CartFeed › an effect follows the other tabs' messages",
        "PASS  Recipe › the screen stays on only while the step-by-step mode is shown",
    ] {
        assert!(text.contains(name), "{name}\n{text}");
    }
}

#[test]
fn test_notifies_through_the_notifications_double() {
    // `Notifications` against `TestNotifications` (45-web-platform.md): the
    // prompt from a click, a show that never prompts, a refused, dismissed
    // or blocked permission, every notification recorded, a click opening
    // the notification's page, and `SystemNotification` as a `Codable`
    // value with its page.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notifying");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", stderr(&out));
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("0 failed"), "{text}");
    for name in [
        "PASS  ExportService › a granted permission shows the notification as given",
        "PASS  ExportService › a notification survives JSON, its page too",
        "PASS  ExportButton › clicking the notification opens its page",
        "PASS  ExportButton › clicking a notification without a page leaves the tab where it is",
    ] {
        assert!(text.contains(name), "{name}\n{text}");
    }
}

#[test]
fn test_expect_throws_checks_the_case_it_names() {
    // A qualified case pattern (`Choice.second`) was read as its type's
    // name, which matched any error.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/throws");
    let out = k(&["test", "--color", "never"], &dir);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}\n{}", stderr(&out));
    assert!(
        text.contains("PASS  Choice › the named case passes"),
        "{text}"
    );
    assert!(
        text.contains("FAIL  Choice › another case of the same error fails"),
        "{text}"
    );
    assert!(
        text.contains("expected an error matching Choice.second, but it threw first"),
        "{text}"
    );
}

/// Reads `child`'s standard output line by line on a thread.
fn lines_of(child: &mut std::process::Child) -> std::sync::mpsc::Receiver<String> {
    use std::io::BufRead;
    let (tx, rx) = std::sync::mpsc::channel();
    let out = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// The lines up to and including the first one containing `needle`, or
/// a failure after 60 seconds.
fn until(rx: &std::sync::mpsc::Receiver<String>, needle: &str) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(line) => {
                let done = line.contains(needle);
                seen.push(line);
                if done {
                    return seen;
                }
            }
            Err(_) => break,
        }
    }
    panic!("no line containing {needle:?}; saw:\n{}", seen.join("\n"));
}

#[test]
fn test_watch_reruns_only_the_affected_tests() {
    let dir = TempDir::new("test_watch");
    fn copy(from: &Path, to: &Path) {
        for e in fs::read_dir(from).unwrap().flatten() {
            let p = e.path();
            let t = to.join(e.file_name());
            if p.is_dir() {
                fs::create_dir_all(&t).unwrap();
                copy(&p, &t);
            } else {
                fs::copy(&p, &t).unwrap();
            }
        }
    }
    copy(&counter(), &dir.0);
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["test", "--watch", "--color", "never"])
        .current_dir(&dir.0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let rx = lines_of(&mut child);
    let first = until(&rx, "watching for changes");
    assert!(first.iter().any(|l| l == "running 9 tests"), "{first:?}");
    // Only CounterView.test.k uses CounterView.
    let view = dir.0.join("src/counter/CounterView.k");
    let text = fs::read_to_string(&view).unwrap();
    fs::write(&view, format!("// edited\n{text}")).unwrap();
    let second = until(&rx, "watching for changes");
    assert!(
        second
            .iter()
            .any(|l| l == "change detected: running the tests of src/counter/CounterView.test.k"),
        "{second:?}"
    );
    assert!(second.iter().any(|l| l == "running 3 tests"), "{second:?}");
    assert!(
        second.iter().all(|l| !l.contains("Counter › ")),
        "the Counter tests don't depend on CounterView: {second:?}"
    );
    // Both test files use Counter.
    let counter_file = dir.0.join("src/counter/Counter.k");
    let text = fs::read_to_string(&counter_file).unwrap();
    fs::write(&counter_file, format!("// edited\n{text}")).unwrap();
    let third = until(&rx, "watching for changes");
    assert!(
        third.iter().any(|l| l
            == "change detected: running the tests of src/counter/Counter.test.k, src/counter/CounterView.test.k"),
        "{third:?}"
    );
    assert!(third.iter().any(|l| l == "running 9 tests"), "{third:?}");
    let _ = child.kill();
    let _ = child.wait();
}

/// docs/design/38-deployment.md, "Static-only apps": a client-rendered app
/// without server-only code builds to `dist/public` only, with the
/// document every URL answers as `index.html`; a `dist/app` left by an
/// earlier build is removed, since it would serve an old version.
#[test]
fn build_of_a_static_only_app_writes_index_html_and_no_app() {
    let dir = TempDir::new("static");
    write(&dir.0, "k.toml", MANIFEST);
    write(
        &dir.0,
        "App.k",
        "@Rendering(.client)\npublic app Demo {\n    metadata {\n        locale(.english)\n    }\n}\n",
    );
    write(&dir.0, "src/site/Home.k", &home("Static"));
    write(&dir.0, "out/app", "an old server build");
    let built = k(&["build", "--out", "out"], &dir.0);
    assert_eq!(built.status.code(), Some(0), "{}", stderr(&built));
    assert!(
        !dir.0.join("out/app").exists(),
        "no dist/app for a static-only app"
    );
    let index = fs::read_to_string(dir.0.join("out/public/index.html")).expect("index.html");
    assert!(
        index.starts_with("<!doctype html><html lang=\"en\">"),
        "{index}"
    );
    assert!(
        index.contains("<div id=\"k-app\"></div>"),
        "an empty mount point: {index}"
    );
    assert!(
        !index.contains("k-data"),
        "a client-rendered document has no k-data: {index}"
    );
    let assets = files_below(&dir.0.join("out/public/assets"));
    for reference in index.split("\"/assets/").skip(1) {
        let path = reference.split('"').next().unwrap_or_default();
        assert!(
            assets.contains(&path.to_owned()),
            "index.html links {path}, which is in dist/public/assets: {assets:?}"
        );
    }
    assert!(
        index.contains("<script type=\"module\" src=\"/assets/app."),
        "{index}"
    );
    assert!(
        index.contains("<link rel=\"stylesheet\" href=\"/assets/app."),
        "{index}"
    );
    let manifest = fs::read_to_string(dir.0.join("out/manifest.json")).expect("manifest");
    assert!(manifest.contains("\"static_only\": true"), "{manifest}");
}

/// A server-rendered app keeps `dist/app` and has no `index.html`.
#[test]
fn build_of_a_server_rendered_app_writes_the_app_and_no_index_html() {
    let dir = TempDir::new("served");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "App.k", "public app Demo {}\n");
    write(&dir.0, "src/site/Home.k", &home("Served"));
    let built = k(&["build", "--out", "out"], &dir.0);
    assert_eq!(built.status.code(), Some(0), "{}", stderr(&built));
    assert!(dir.0.join("out/app").is_file());
    assert!(!dir.0.join("out/public/index.html").exists());
}

//! Persistence in generated code (`docs/compiler/db.md` §4–§10): queries
//! become numbered statements in the build's database manifest (never SQL
//! in JavaScript), locked names come from `schema.lock`, migrations replay
//! into both dialects, and what can't be translated, or migrations that
//! don't produce the entities' tables, fail the build with a message
//! naming the problem.

use k_codegen::{CodegenError, Options, Output, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};

/// The `k.toml` of the test project: a package named `demo`.
const TOML: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

/// The entity the test repositories query.
const NOTE: &str = "import k.crypto.Uuid\n\npublic entity Note {\n    @Id let id: Uuid\n    @Index let owner: String\n    @Unique var title: String\n    var isPinned: Bool = false\n    @Version var revision: Int = 1\n}\n";

/// The migration that creates [`NOTE`]'s table, so the build has a schema.
const MIGRATION: &str = "migration Init {\n    func up(schema: Schema) {\n        schema.createTable(name: \"note\") { table =>\n            table.column(name: \"id\", type: .uuid).primaryKey(true)\n            table.column(name: \"owner\", type: .text).indexed(true)\n            table.column(name: \"title\", type: .text).unique(true)\n            table.column(name: \"is_pinned\", type: .boolean).defaultValue(.boolean(false))\n            table.column(name: \"revision\", type: .integer)\n        }\n    }\n}\n";

/// Analyses `files` and generates them with the default options.
///
/// # Panics
///
/// If a path is invalid, the analysis is cancelled, or the program has an
/// error: every case must check cleanly.
fn generate_files(files: &[(&str, &str)]) -> Result<Output, CodegenError> {
    let files: Vec<(ProjectPath, String)> = files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).expect("path"), (*t).to_owned()))
        .collect();
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("analysis");
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "the case must check cleanly: {errors:?}");
    generate(&analysis, &Options::default())
}

/// The source of `NoteRepository` with the members `body`.
fn repository(body: &str) -> String {
    format!("public repository NoteRepository for Note {{\n{body}\n}}\n")
}

/// Generates the test project with the repository source `repo`.
fn program(repo: &str) -> Result<Output, CodegenError> {
    generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Note.k", NOTE),
        ("src/demo/NoteRepository.k", repo),
        ("migrations/0001_init.k", MIGRATION),
    ])
}

/// The SQLite text of every statement of the build, by number.
///
/// # Panics
///
/// If the build has no database manifest.
fn sqlite(out: &Output) -> Vec<String> {
    out.database
        .as_ref()
        .expect("a database manifest")
        .statements
        .iter()
        .map(|s| s.sqlite.clone())
        .collect()
}

/// Every server module's contents, concatenated.
fn server_js(out: &Output) -> String {
    out.server.iter().map(|f| f.contents.as_str()).collect()
}

#[test]
fn a_query_becomes_a_numbered_statement_in_both_dialects() {
    let out = program(&repository(
        "    func pinned(owner: String) async -> List<Note> {\n        return await notes\n            .filter { note => note.owner == owner and note.isPinned }\n            .sortedByDescending { note => note.revision }\n            .limit(10)\n    }",
    ))
    .expect("generates");
    let db = out.database.as_ref().expect("a database manifest");
    let i = db
        .statements
        .iter()
        .position(|s| s.sqlite.contains("ORDER BY"))
        .expect("the query's statement");
    assert_eq!(
        db.statements[i].sqlite,
        "SELECT \"id\", \"owner\", \"title\", \"is_pinned\", \"revision\" FROM \"note\" WHERE \"owner\" = ?1 AND \"is_pinned\" = ?2 ORDER BY \"revision\" DESC LIMIT 10"
    );
    assert_eq!(
        db.statements[i].postgres,
        "SELECT \"id\", \"owner\", \"title\", \"is_pinned\", \"revision\" FROM \"note\" WHERE \"owner\" = $1 AND \"is_pinned\" = $2 ORDER BY \"revision\" DESC NULLS LAST LIMIT 10"
    );
    // The JavaScript names the statement by number and binds every value,
    // the constant `true` of `note.isPinned` included.
    let js = server_js(&out);
    assert!(
        js.contains(&format!("$s.dbQuery(this.$db, 0, {i}, ")),
        "{js}"
    );
    assert!(js.contains("[$s.dbc.bool, true]"), "{js}");
    assert!(!js.contains("SELECT"), "no SQL in JavaScript");
    assert!(!js.contains("\"note\""), "no table names in JavaScript");
}

#[test]
fn the_inherited_writes_have_their_statements() {
    let out = program(&repository("")).expect("generates");
    let all = sqlite(&out);
    for expected in [
        "INSERT INTO \"note\" (\"id\", \"owner\", \"title\", \"is_pinned\", \"revision\") VALUES (?1, ?2, ?3, ?4, ?5)",
        // @Version: a conditional update that bumps the revision.
        "UPDATE \"note\" SET \"owner\" = ?1, \"title\" = ?2, \"is_pinned\" = ?3, \"revision\" = (\"revision\" + ?4) WHERE \"id\" = ?5 AND \"revision\" = ?6",
        "DELETE FROM \"note\" WHERE \"id\" = ?1",
    ] {
        assert!(all.iter().any(|s| s == expected), "{expected}\nin {all:#?}");
    }
    // insert(all:) has batch statements of 64, 16, 4 and 1 rows.
    let batches = all.iter().filter(|s| s.starts_with("INSERT")).count();
    assert_eq!(batches, 4, "{all:#?}");
    let db = out.database.as_ref().expect("manifest");
    assert_eq!(db.unique.len(), 1);
    assert_eq!(db.unique[0].index, "note_title_key");
    assert_eq!(db.unique[0].field, "title");
    assert_eq!(db.migrations.len(), 1);
    assert_eq!(db.migrations[0].name, "0001_init");
    assert!(db.migrations[0].postgres[0].starts_with("CREATE TABLE \"note\" (\"id\" uuid"));
}

#[test]
fn identical_queries_share_one_statement() {
    let out = program(&repository(
        "    func a(owner: String) async -> List<Note> {\n        return await notes.filter { note => note.owner == owner }\n    }\n    func b(who: String) async -> List<Note> {\n        return await notes.filter { n => n.owner == who }\n    }",
    ))
    .expect("generates");
    let all = sqlite(&out);
    let selects = all
        .iter()
        .filter(|s| s.ends_with("WHERE \"owner\" = ?1"))
        .count();
    assert_eq!(selects, 1, "{all:#?}");
}

#[test]
fn locked_names_from_schema_lock_are_used() {
    let lock = "migrations = [\"0001_init\"]\n\n[tables.memo]\nentity = \"demo.Note\"\ncolumns = [\n    { field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true },\n    { field = \"owner\", name = \"owner\", type = \"text\", indexed = true },\n    { field = \"title\", name = \"heading\", type = \"text\", unique = true },\n    { field = \"isPinned\", name = \"is_pinned\", type = \"boolean\", default = false },\n    { field = \"revision\", name = \"revision\", type = \"integer\" },\n]\n";
    let migration = MIGRATION
        .replace("name: \"note\"", "name: \"memo\"")
        .replace("name: \"title\"", "name: \"heading\"");
    let out = generate_files(&[
        ("k.toml", TOML),
        ("schema.lock", lock),
        ("src/demo/Note.k", NOTE),
        (
            "src/demo/NoteRepository.k",
            &repository(
                "    func titled(t: String) async -> Note? {\n        return await notes.first { note => note.title == t }\n    }",
            ),
        ),
        ("migrations/0001_init.k", &migration),
    ])
    .expect("generates");
    let all = sqlite(&out);
    assert!(
        all.iter().any(|s| s
            == "SELECT \"id\", \"owner\", \"heading\", \"is_pinned\", \"revision\" FROM \"memo\" WHERE \"heading\" = ?1 LIMIT 1"),
        "{all:#?}"
    );
    let db = out.database.as_ref().expect("manifest");
    assert_eq!(db.unique[0].index, "memo_heading_key");
}

/// The message of the build error `out` must be; anything else becomes a
/// text no assertion matches, so the test fails showing it.
fn unsupported(out: Result<Output, CodegenError>) -> String {
    match out {
        Err(CodegenError::Unsupported { what }) => what,
        Err(e) => format!("expected a persistence build error, got {e}"),
        Ok(_) => "expected a persistence build error, got output".to_owned(),
    }
}

#[test]
fn computing_with_the_row_is_a_build_error() {
    let what = unsupported(program(&repository(
        "    func next(r: Int) async -> List<Note> {\n        return await notes.filter { note => note.revision == note.revision + r }\n    }",
    )));
    assert!(what.contains("NoteRepository.k:3"), "{what}");
    assert!(
        what.contains("only a field of the row can be compared"),
        "{what}"
    );
}

#[test]
fn a_sort_key_must_be_a_field() {
    let what = unsupported(program(&repository(
        "    func odd() async -> List<Note> {\n        return await notes.sortedBy { note => note.title.count }\n    }",
    )));
    assert!(
        what.contains("a sort key must be a field of the row"),
        "{what}"
    );
}

#[test]
fn migrations_that_dont_create_the_entities_columns_fail_the_build() {
    let partial = MIGRATION.replace(
        "            table.column(name: \"revision\", type: .integer)\n",
        "",
    );
    let what = unsupported(generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Note.k", NOTE),
        ("migrations/0001_init.k", &partial),
    ]));
    assert!(
        what.contains("`demo.Note.revision` is stored in column `note.revision`, which the migrations don't create"),
        "{what}"
    );
    let typo = MIGRATION.replace(
        "table.column(name: \"title\", type: .text)",
        "table.column(name: \"title\", type: .integer)",
    );
    let what = unsupported(generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Note.k", NOTE),
        ("migrations/0001_init.k", &typo),
    ]));
    assert!(
        what.contains("is a `text` but the migrations make `note.title` a `integer` column"),
        "{what}"
    );
}

#[test]
fn a_migration_that_does_not_replay_fails_the_build() {
    let second = "migration Broken {\n    func up(schema: Schema) {\n        schema.alterTable(name: \"notes\") { table =>\n            table.dropColumn(name: \"title\")\n        }\n    }\n}\n";
    let what = unsupported(generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Note.k", NOTE),
        ("migrations/0001_init.k", MIGRATION),
        ("migrations/0002_broken.k", second),
    ]));
    assert!(what.contains("migrations/0002_broken.k"), "{what}");
    assert!(what.contains("no table `notes`"), "{what}");
}

#[test]
fn a_program_without_persistence_has_no_database_manifest() {
    let out = generate_files(&[
        ("k.toml", TOML),
        ("src/demo/a.k", "func f() -> Int {\n    return 1\n}\n"),
    ])
    .expect("generates");
    assert!(out.database.is_none());
}

#[test]
fn a_migration_that_computes_fails_the_build() {
    let computing = "migration Computes {\n    func up(schema: Schema) {\n        let name = \"note\"\n        schema.dropTable(name: name)\n    }\n}\n";
    let what = unsupported(generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Note.k", NOTE),
        ("migrations/0001_init.k", MIGRATION),
        ("migrations/0002_computes.k", computing),
    ]));
    assert!(what.contains("migrations/0002_computes.k"), "{what}");
    assert!(what.contains("can't run at migration time"), "{what}");
}

#[test]
fn a_double_field_has_no_column_type() {
    let entity = "import k.crypto.Uuid\n\npublic entity Reading {\n    @Id let id: Uuid\n    let value: Double\n}\n";
    let what = unsupported(generate_files(&[
        ("k.toml", TOML),
        ("src/demo/Reading.k", entity),
    ]));
    assert!(
        what.contains("`demo.Reading.value`: a `Double` field has no column type"),
        "{what}"
    );
}

//! Property: **SQL generation never interpolates values**, end to end
//! (`docs/compiler/db.md` §4–§6, `docs/compiler/testing.md`, "Stage 3
//! properties"). Generated repositories filter, search, sort and limit with
//! hostile constants (quotes, `--`, `;`, `%`, `_`, `\`, braces, Unicode) and
//! captured parameters; for every program:
//!
//! * every statement of the build's database manifest, in both dialects,
//!   holds no string literal (other than `LIKE`'s `ESCAPE '\'`) and no
//!   number other than a source `limit`, once quoted identifiers and
//!   numbered parameters are taken out, and no constant of the program;
//! * the server JavaScript names statements by number and holds no SQL;
//! * no client file holds SQL or the repository.
//!
//! `PROPTEST_CASES=N` sets the number of cases (default 48; each is a whole
//! analysis and generation).

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use k_codegen::{Options, check_client_bundle, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use proptest::prelude::*;

/// The `k.toml` of the test project: a package named `demo`.
const TOML: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

/// The entity the generated repositories query: an id, an indexed owner, a
/// unique title, a flag and a version.
const NOTE: &str = "import k.crypto.Uuid\n\npublic entity Note {\n    @Id let id: Uuid\n    @Index let owner: String\n    @Unique var title: String\n    var isPinned: Bool = false\n    @Version var revision: Int = 1\n}\n";

/// The migration that creates [`NOTE`]'s table, so the build has a schema.
const MIGRATION: &str = "migration Init {\n    func up(schema: Schema) {\n        schema.createTable(name: \"note\") { table =>\n            table.column(name: \"id\", type: .uuid).primaryKey(true)\n            table.column(name: \"owner\", type: .text).indexed(true)\n            table.column(name: \"title\", type: .text).unique(true)\n            table.column(name: \"is_pinned\", type: .boolean).defaultValue(.boolean(false))\n            table.column(name: \"revision\", type: .integer)\n        }\n    }\n}\n";

/// Constants that would break or change a statement if interpolated.
const HOSTILE: &[&str] = &[
    "'; DROP TABLE note; --",
    "' OR '1'='1",
    "\"",
    "%",
    "_",
    "\\",
    "a%b_c",
    "{{braces}",
    "?1",
    "$1",
    "\u{e9}\u{1F642}",
    "",
    "--",
    "x LIMIT 9999 --",
];

/// One conjunct of a filter, over the row `note` and the parameter `p`.
#[derive(Clone, Debug)]
enum Cond {
    /// `note.owner == "…"` (or `!=`).
    OwnerIs(usize, bool),
    /// `note.title.contains("…")` and friends.
    Title(usize, usize),
    /// `note.revision op n`.
    Revision(usize, i64),
    /// `note.isPinned == true/false`.
    Pinned(bool),
    /// `note.owner == p` (a captured parameter).
    OwnerParam,
}

/// `text` escaped for a K string literal (backslashes and quotes).
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The K source of one filter conjunct.
fn cond_text(c: &Cond) -> String {
    match c {
        Cond::OwnerIs(i, eq) => format!(
            "note.owner {} \"{}\"",
            if *eq { "==" } else { "!=" },
            escape(HOSTILE[*i])
        ),
        Cond::Title(m, i) => format!(
            "note.title.{}(\"{}\")",
            ["contains", "startsWith", "endsWith"][*m],
            escape(HOSTILE[*i])
        ),
        Cond::Revision(op, n) => format!("note.revision {} {n}", ["==", "<", ">=", "!="][*op]),
        Cond::Pinned(b) => format!("note.isPinned == {b}"),
        Cond::OwnerParam => "note.owner == p".to_owned(),
    }
}

/// A strategy for one filter conjunct over the [`HOSTILE`] constants.
fn cond() -> impl Strategy<Value = Cond> {
    let h = 0..HOSTILE.len();
    prop_oneof![
        (h.clone(), any::<bool>()).prop_map(|(i, eq)| Cond::OwnerIs(i, eq)),
        (0usize..3, h).prop_map(|(m, i)| Cond::Title(m, i)),
        (0usize..4, -3i64..1000).prop_map(|(op, n)| Cond::Revision(op, n)),
        any::<bool>().prop_map(Cond::Pinned),
        Just(Cond::OwnerParam),
    ]
}

/// A query: conjuncts, an optional sort, an optional limit.
#[derive(Clone, Debug)]
struct Query {
    /// The conjuncts of the filter, joined with `and`.
    conds: Vec<Cond>,
    /// A sort by revision: `Some(true)` descending, `Some(false)`
    /// ascending.
    sort: Option<bool>,
    /// A constant `limit`.
    limit: Option<u32>,
}

/// A strategy for one query: one to three conjuncts, an optional sort and
/// an optional limit below 100.
fn query() -> impl Strategy<Value = Query> {
    (
        prop::collection::vec(cond(), 1..4),
        prop::option::of(any::<bool>()),
        prop::option::of(1u32..100),
    )
        .prop_map(|(conds, sort, limit)| Query { conds, sort, limit })
}

/// The K source of a `NoteRepository` with one function `q<i>(p:)` per
/// query, each a `notes.filter { … }` chain.
fn repository(queries: &[Query]) -> String {
    let mut out = String::from("public repository NoteRepository for Note {\n");
    for (i, q) in queries.iter().enumerate() {
        let filter: Vec<String> = q.conds.iter().map(cond_text).collect();
        out.push_str(&format!(
            "    func q{i}(p: String) async -> List<Note> {{\n        return await notes\n            .filter {{ note => {} }}\n",
            filter.join(" and ")
        ));
        if let Some(desc) = q.sort {
            out.push_str(&format!(
                "            .{} {{ note => note.revision }}\n",
                if desc {
                    "sortedByDescending"
                } else {
                    "sortedBy"
                }
            ));
        }
        if let Some(n) = q.limit {
            out.push_str(&format!("            .limit({n})\n"));
        }
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    out
}

/// Takes quoted identifiers and parameters out of `sql` and returns the
/// rest, or why the text is malformed.
fn skeleton(sql: &str) -> Result<String, String> {
    let mut rest = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                        }
                        Some('"') => break,
                        Some(_) => {}
                        None => return Err(format!("unterminated identifier: {sql}")),
                    }
                }
                rest.push('I');
            }
            '?' | '$' => {
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                rest.push('P');
            }
            c => rest.push(c),
        }
    }
    Ok(rest)
}

/// The proptest configuration: proptest's own (which reads
/// `PROPTEST_CASES`) when that variable is set, otherwise 48 cases.
fn config() -> ProptestConfig {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        ProptestConfig::default()
    } else {
        ProptestConfig::with_cases(48)
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn repository_statements_interpolate_no_value(queries in prop::collection::vec(query(), 1..4)) {
        let repo = repository(&queries);
        let files: Vec<(ProjectPath, String)> = [
            ("k.toml", TOML),
            ("src/demo/Note.k", NOTE),
            ("src/demo/NoteRepository.k", repo.as_str()),
            ("migrations/0001_init.k", MIGRATION),
        ]
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).unwrap(), (*t).to_owned()))
        .collect();
        let host = AnalysisHost::with_files(SemaOptions::new(), files);
        let analysis = host.analysis();
        let errors: Vec<String> = analysis
            .diagnostics()
            .unwrap()
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        prop_assert!(errors.is_empty(), "{errors:?}\n{repo}");
        let out = generate(&analysis, &Options::default()).map_err(|e| TestCaseError::fail(format!("{e}\n{repo}")))?;
        let db = out.database.as_ref().expect("a database manifest");
        let limits: Vec<String> = queries.iter().filter_map(|q| q.limit.map(|n| n.to_string())).collect();
        for st in &db.statements {
            for sql in [&st.sqlite, &st.postgres] {
                let rest = skeleton(sql).map_err(TestCaseError::fail)?;
                let rest = rest.replace(" ESCAPE '\\'", " ESCAPE");
                prop_assert!(!rest.contains('\''), "a string literal in {sql}\n{repo}");
                for word in rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
                    if word.chars().any(|c| c.is_ascii_digit()) {
                        prop_assert!(
                            matches!(word, "1" | "0" | "-1") || limits.iter().any(|l| l == word),
                            "number {word:?} in {sql}\n{repo}"
                        );
                    }
                }
                for h in HOSTILE.iter().filter(|h| h.len() > 2) {
                    prop_assert!(!sql.contains(*h), "constant {h:?} in {sql}\n{repo}");
                }
            }
        }
        let js: String = out.server.iter().map(|f| f.contents.as_str()).collect();
        prop_assert!(!js.contains("SELECT ") && !js.contains("\"note\""), "SQL in the server JavaScript\n{repo}");
        let split = check_client_bundle(&out, &["NoteRepository", "SELECT "]);
        prop_assert!(split.is_empty(), "{split:?}\n{repo}");
    }
}

//! The web platform page (`docs/book/src/reference/web-platform.md`) and its
//! data, `docs/book/data/web-apis.toml`: which browser APIs K wraps, how,
//! and which it doesn't (`docs/design/45-web-platform.md`, "Inventory";
//! `docs/compiler/documentation.md` §1).
//!
//! * every row is well formed: a known status, a tier on planned rows, a K
//!   API and a side on supported and partial ones, no real-browser checks on
//!   rows that aren't built, a design topic and heading that exist, a guide
//!   page that exists and a feature id that `k_sema::features` lists as
//!   planned;
//! * every K API a row names resolves in the standard library (a
//!   declaration, `k.device.Clipboard`) or is one of the universal element
//!   events (`event:appeared`);
//! * every file of `crates/k_runtime/js/runtime` is claimed by a row, or
//!   listed under `[internal]` with why it wraps no browser API, so a new
//!   binding fails here until the page says what it covers; a claim of a
//!   file that doesn't exist fails too;
//! * every "verified in Chromium/Firefox" claim names a `#[test]` of
//!   `crates/k_cli/tests/chrome.rs` or `firefox.rs`;
//! * the page's generated part, between `<!-- gen:web-platform … -->` and
//!   `<!-- /gen -->`, is what the data renders
//!   (`K_BLESS=1 cargo test -p k_sema --test it -- web_platform::` rewrites
//!   it).

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::mdbook;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// The data file, relative to the repository root.
const DATA: &str = "docs/book/data/web-apis.toml";
/// The page, relative to the book's source.
const PAGE: &str = "reference/web-platform.md";
/// The runtime's JavaScript, relative to the repository root.
const RUNTIME: &str = "crates/k_runtime/js/runtime";
const GEN_START: &str = "<!-- gen:web-platform: generated from docs/book/data/web-apis.toml; regenerate with K_BLESS=1 cargo test -p k_sema --test it -- web_platform:: -->";
const GEN_END: &str = "<!-- /gen -->";

/// The statuses a row may have, in the summary's column order, with the
/// word the page shows.
const STATUSES: &[(&str, &str)] = &[
    ("supported", "Supported"),
    ("partial", "Partial"),
    ("planned", "Planned"),
    ("not-planned", "Not planned"),
];

/// Where a row's code runs.
const SIDES: &[(&str, &str)] = &[
    ("client", "browser"),
    ("server", "server"),
    ("both", "both"),
];

/// One API (a row of the data).
struct Api {
    area: String,
    name: String,
    status: String,
    tier: Option<i64>,
    k: Vec<String>,
    side: Option<String>,
    runtime: Vec<String>,
    chromium: Vec<String>,
    firefox: Vec<String>,
    design: String,
    guide: Option<String>,
    feature: Option<String>,
    note: String,
}

/// One area: a heading of the page.
struct Area {
    name: String,
    guide: Option<String>,
    intro: Option<String>,
}

/// The parsed data file.
struct Data {
    areas: Vec<Area>,
    apis: Vec<Api>,
    /// Runtime files that wrap no browser API, with why.
    internal: BTreeMap<String, String>,
}

fn repo() -> PathBuf {
    mdbook::repo()
}

fn strings(table: &toml::Table, key: &str, row: &str) -> Vec<String> {
    match table.get(key) {
        None => Vec::new(),
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap_or_else(|| panic!("{DATA}: {row}: `{key}` has a non-string item"))
                    .to_owned()
            })
            .collect(),
        Some(_) => panic!("{DATA}: {row}: `{key}` isn't a list of strings"),
    }
}

fn string(table: &toml::Table, key: &str, row: &str) -> Option<String> {
    table.get(key).map(|v| {
        v.as_str()
            .unwrap_or_else(|| panic!("{DATA}: {row}: `{key}` isn't a string"))
            .trim()
            .to_owned()
    })
}

/// The keys a row may have; any other is a typo.
const API_KEYS: &[&str] = &[
    "name", "status", "tier", "k", "side", "runtime", "chromium", "firefox", "design", "guide",
    "feature", "note",
];

fn data() -> Data {
    let text = fs::read_to_string(repo().join(DATA))
        .unwrap_or_else(|e| panic!("{DATA} can't be read: {e}"));
    let table: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("{DATA} isn't TOML: {e}"));
    let mut internal = BTreeMap::new();
    if let Some(t) = table.get("internal") {
        let t = t
            .as_table()
            .unwrap_or_else(|| panic!("{DATA}: `internal` isn't a table"));
        for (file, why) in t {
            let why = why
                .as_str()
                .unwrap_or_else(|| panic!("{DATA}: internal.{file} isn't a string"));
            internal.insert(file.clone(), why.to_owned());
        }
    }
    let mut areas = Vec::new();
    let mut apis = Vec::new();
    let list = table
        .get("area")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{DATA}: no `[[area]]`"));
    for area in list {
        let area = area
            .as_table()
            .unwrap_or_else(|| panic!("{DATA}: an `[[area]]` isn't a table"));
        let name = string(area, "name", "an area")
            .unwrap_or_else(|| panic!("{DATA}: an area has no name"));
        areas.push(Area {
            name: name.clone(),
            guide: string(area, "guide", &name),
            intro: string(area, "intro", &name),
        });
        let rows = area
            .get("api")
            .and_then(toml::Value::as_array)
            .unwrap_or_else(|| panic!("{DATA}: area {name:?} has no `[[area.api]]`"));
        for row in rows {
            let row = row
                .as_table()
                .unwrap_or_else(|| panic!("{DATA}: an api of {name:?} isn't a table"));
            let api_name = string(row, "name", &name)
                .unwrap_or_else(|| panic!("{DATA}: an api of {name:?} has no name"));
            let label = format!("{name} / {api_name}");
            for key in row.keys() {
                assert!(
                    API_KEYS.contains(&key.as_str()),
                    "{DATA}: {label}: unknown key `{key}` (one of {API_KEYS:?})"
                );
            }
            apis.push(Api {
                area: name.clone(),
                status: string(row, "status", &label)
                    .unwrap_or_else(|| panic!("{DATA}: {label}: no status")),
                tier: row.get("tier").map(|t| {
                    t.as_integer()
                        .unwrap_or_else(|| panic!("{DATA}: {label}: `tier` isn't an integer"))
                }),
                k: strings(row, "k", &label),
                side: string(row, "side", &label),
                runtime: strings(row, "runtime", &label),
                chromium: strings(row, "chromium", &label),
                firefox: strings(row, "firefox", &label),
                design: string(row, "design", &label)
                    .unwrap_or_else(|| panic!("{DATA}: {label}: no design topic")),
                guide: string(row, "guide", &label),
                feature: string(row, "feature", &label),
                note: string(row, "note", &label).unwrap_or_default(),
                name: api_name,
            });
        }
    }
    Data {
        areas,
        apis,
        internal,
    }
}

impl Api {
    fn label(&self) -> String {
        format!("{} / {}", self.area, self.name)
    }

    fn built(&self) -> bool {
        matches!(self.status.as_str(), "supported" | "partial")
    }
}

/// The ids of a design topic's headings (GitHub's, which `book_links.rs`
/// also uses for `docs/design`).
fn design_anchors(file: &str) -> Option<Vec<String>> {
    let text = fs::read_to_string(repo().join("docs/design").join(file)).ok()?;
    Some(mdbook::heading_ids(&mdbook::parse(&text)))
}

/// Every row is well formed.
#[test]
fn web_api_rows_are_well_formed() {
    use k_sema::features::{Status, lookup};
    let data = data();
    let mut problems = Vec::new();
    let mut names = BTreeSet::new();
    let mut anchors: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    for api in &data.apis {
        let label = api.label();
        if !names.insert(api.name.clone()) {
            problems.push(format!("{label}: a second row of this name"));
        }
        if !STATUSES.iter().any(|(s, _)| *s == api.status) {
            problems.push(format!(
                "{label}: status `{}` isn't one of {:?}",
                api.status,
                STATUSES.iter().map(|(s, _)| *s).collect::<Vec<_>>()
            ));
        }
        match (api.status.as_str(), api.tier) {
            ("planned", Some(2 | 3)) => {}
            ("planned", None) if api.feature.is_some() => {}
            ("planned", _) => problems.push(format!(
                "{label}: a planned row needs `tier = 2` or `3`, or the `feature` that tracks it"
            )),
            (_, Some(_)) => problems.push(format!("{label}: only a planned row has a tier")),
            _ => {}
        }
        if api.built() {
            if api.k.is_empty() {
                problems.push(format!(
                    "{label}: a {} row names its K API (`k`)",
                    api.status
                ));
            }
            if api.side.is_none() {
                problems.push(format!(
                    "{label}: a {} row says where it runs (`side`)",
                    api.status
                ));
            }
        } else if !api.chromium.is_empty() || !api.firefox.is_empty() {
            problems.push(format!(
                "{label}: a {} row can't be verified in a browser",
                api.status
            ));
        }
        if let Some(side) = &api.side
            && !SIDES.iter().any(|(s, _)| s == side)
        {
            problems.push(format!(
                "{label}: side `{side}` isn't client, server or both"
            ));
        }
        if api.note.is_empty() && api.status != "supported" {
            problems.push(format!("{label}: a {} row says why in `note`", api.status));
        }
        let (file, anchor) = api
            .design
            .split_once('#')
            .map_or((api.design.as_str(), None), |(f, a)| (f, Some(a)));
        let ids = anchors
            .entry(file.to_owned())
            .or_insert_with(|| design_anchors(file));
        match (ids, anchor) {
            (None, _) => problems.push(format!("{label}: no design topic docs/design/{file}")),
            (Some(ids), Some(anchor)) if !ids.iter().any(|i| i == anchor) => {
                problems.push(format!(
                    "{label}: docs/design/{file} has no heading `#{anchor}`"
                ));
            }
            _ => {}
        }
        if let Some(feature) = &api.feature {
            match lookup(feature) {
                Some(f) if f.status == Status::Planned => {}
                Some(_) => problems.push(format!(
                    "{label}: feature `{feature}` isn't planned any more"
                )),
                None => problems.push(format!(
                    "{label}: no feature `{feature}` in k_sema::features"
                )),
            }
        }
    }
    for area in &data.areas {
        if !data.apis.iter().any(|a| a.area == area.name) {
            problems.push(format!("area {:?} has no rows", area.name));
        }
    }
    for guide in data
        .apis
        .iter()
        .filter_map(|a| a.guide.as_ref())
        .chain(data.areas.iter().filter_map(|a| a.guide.as_ref()))
    {
        let page = guide.split('#').next().unwrap_or(guide);
        if !mdbook::book_src().join(page).is_file() {
            problems.push(format!("guide `{guide}` isn't a page of the book"));
        }
    }
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "{DATA}:\n{}", problems.join("\n"));
}

/// Why the K API `name` doesn't resolve, or `None` when it does.
fn unresolved(name: &str, stdlib: &k_sema::stdlib::Stdlib) -> Option<String> {
    use k_sema::stdlib::elements::UNIVERSAL_EVENTS;
    if let Some(event) = name.strip_prefix("event:") {
        return (!UNIVERSAL_EVENTS.iter().any(|e| e.name == event))
            .then(|| format!("`{event}` isn't an event every element has"));
    }
    if !name.starts_with("k.") {
        return Some(format!(
            "`{name}` isn't a qualified standard library name (`k.module.Type`) or `event:name`"
        ));
    }
    stdlib
        .lookup(name)
        .is_none()
        .then(|| format!("`{name}` isn't a declaration of the standard library"))
}

/// Every K API a row names exists in the standard library.
#[test]
fn web_api_rows_name_standard_library_declarations() {
    let stdlib = k_sema::stdlib::Stdlib::get();
    let mut problems = Vec::new();
    for api in data().apis {
        for name in &api.k {
            if let Some(problem) = unresolved(name, stdlib) {
                problems.push(format!("{}: {problem}", api.label()));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{DATA} names K APIs that don't exist:\n{}",
        problems.join("\n")
    );
}

/// Every runtime file is claimed by a row or listed as internal.
#[test]
fn every_runtime_binding_is_on_the_web_platform_page() {
    let data = data();
    let files: BTreeSet<String> = fs::read_dir(repo().join(RUNTIME))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".js"))
        .collect();
    let mut claimed: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for api in &data.apis {
        for file in &api.runtime {
            claimed.entry(file).or_default().push(api.label());
        }
    }
    let mut problems = Vec::new();
    for file in &files {
        let rows = claimed.get(file.as_str());
        match (rows, data.internal.contains_key(file)) {
            (None, false) => problems.push(format!(
                "{RUNTIME}/{file}: no row of {DATA} claims it (`runtime = [\"{file}\"]` on the APIs \
                 it wraps), and `[internal]` doesn't list it"
            )),
            (Some(rows), true) => problems.push(format!(
                "{RUNTIME}/{file}: both claimed ({}) and listed under `[internal]`",
                rows.join(", ")
            )),
            _ => {}
        }
    }
    for (file, rows) in &claimed {
        if !files.contains(*file) {
            problems.push(format!(
                "{}: claims {file}, which isn't in {RUNTIME}",
                rows.join(", ")
            ));
        }
    }
    for (file, why) in &data.internal {
        if !files.contains(file) {
            problems.push(format!("[internal] lists {file}, which isn't in {RUNTIME}"));
        }
        if why.trim().is_empty() {
            problems.push(format!(
                "[internal] {file}: say why it wraps no browser API"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The `#[test]` functions of a test file of `k_cli`.
fn tests_in(file: &str) -> BTreeSet<String> {
    let text = fs::read_to_string(repo().join("crates/k_cli/tests").join(file)).unwrap();
    let test_fn = regex::Regex::new(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([a-z0-9_]+)").unwrap();
    test_fn
        .captures_iter(&text)
        .map(|c| c[1].to_owned())
        .collect()
}

/// Every verified-in-an-engine claim names a real-browser test of that engine.
#[test]
fn web_api_engine_claims_name_real_browser_tests() {
    let chrome = tests_in("chrome.rs");
    let firefox = tests_in("firefox.rs");
    let mut problems = Vec::new();
    for api in data().apis {
        for (engine, claims, tests, file) in [
            ("chromium", &api.chromium, &chrome, "chrome.rs"),
            ("firefox", &api.firefox, &firefox, "firefox.rs"),
        ] {
            for test in claims {
                if !tests.contains(test) {
                    problems.push(format!(
                        "{}: `{engine}` names `{test}`, which isn't a #[test] of crates/k_cli/tests/{file}",
                        api.label()
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// ---------------------------------------------------------------------------
// The page

/// `name` (`k.device.Clipboard`, `event:appeared`) as the page shows it,
/// linked to its reference section.
fn k_link(name: &str) -> String {
    if let Some(event) = name.strip_prefix("event:") {
        return format!("[`{event}`](../apps/environment.md#element-events) event");
    }
    let (module, declaration) = name.rsplit_once('.').unwrap();
    let page = module.replace('.', "-");
    format!(
        "[`{declaration}`](stdlib/{page}.md#{})",
        mdbook::heading_id(declaration)
    )
}

/// The status as the page shows it.
fn status_word(api: &Api) -> String {
    let word = STATUSES
        .iter()
        .find(|(s, _)| *s == api.status)
        .map_or(api.status.as_str(), |(_, w)| *w);
    match api.tier {
        Some(tier) => format!("{word} (Tier {tier})"),
        None => word.to_owned(),
    }
}

fn verified(api: &Api) -> String {
    let mut engines = Vec::new();
    if !api.chromium.is_empty() {
        engines.push("Chromium");
    }
    if !api.firefox.is_empty() {
        engines.push("Firefox");
    }
    if engines.is_empty() {
        "—".to_owned()
    } else {
        engines.join(", ")
    }
}

/// A table cell: no line breaks or bare pipes.
fn cell(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

/// `guide` (relative to the book's source) as a link from `reference/`.
fn guide_link(guide: &str) -> String {
    format!("../{guide}")
}

fn page_title(guide: &str) -> String {
    let page = guide.split('#').next().unwrap_or(guide);
    let text = fs::read_to_string(mdbook::book_src().join(page)).unwrap_or_default();
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or(page)
        .trim()
        .to_owned()
}

/// The generated part of the page.
fn render(data: &Data) -> String {
    let mut out = String::new();
    // Totals.
    let count = |area: Option<&str>, status: &str| {
        data.apis
            .iter()
            .filter(|a| area.is_none_or(|n| a.area == n) && a.status == status)
            .count()
    };
    out.push_str("\n## Summary\n\n| Area |");
    for (_, word) in STATUSES {
        out.push_str(&format!(" {word} |"));
    }
    out.push_str("\n|---|");
    for _ in STATUSES {
        out.push_str("--:|");
    }
    out.push('\n');
    for area in &data.areas {
        out.push_str(&format!(
            "| [{}](#{}) |",
            area.name,
            mdbook::heading_id(&area.name)
        ));
        for (status, _) in STATUSES {
            out.push_str(&format!(" {} |", count(Some(&area.name), status)));
        }
        out.push('\n');
    }
    out.push_str("| **All** |");
    for (status, _) in STATUSES {
        out.push_str(&format!(" **{}** |", count(None, status)));
    }
    out.push('\n');

    for area in &data.areas {
        out.push_str(&format!("\n## {}\n\n", area.name));
        if let Some(intro) = &area.intro {
            out.push_str(&cell(intro));
            out.push_str("\n\n");
        }
        if let Some(guide) = &area.guide {
            out.push_str(&format!(
                "Guide: [{}]({}).\n\n",
                page_title(guide),
                guide_link(guide)
            ));
        }
        out.push_str(
            "| API | Status | Kobra | Runs on | Verified in | Notes |\n|---|---|---|---|---|---|\n",
        );
        let rows: Vec<&Api> = data.apis.iter().filter(|a| a.area == area.name).collect();
        for api in &rows {
            let k = if api.k.is_empty() {
                "—".to_owned()
            } else {
                api.k
                    .iter()
                    .map(|n| k_link(n))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let side = api.side.as_deref().map_or("—", |s| {
                SIDES.iter().find(|(k, _)| *k == s).map_or(s, |(_, w)| *w)
            });
            let mut note = cell(&api.note);
            if let Some(feature) = &api.feature {
                if !note.is_empty() {
                    note.push(' ');
                }
                note.push_str(&format!(
                    "Feature `{feature}` on [Design status](design-status.md)."
                ));
            }
            if let Some(guide) = &api.guide {
                if !note.is_empty() {
                    note.push(' ');
                }
                note.push_str(&format!(
                    "See [{}]({}).",
                    page_title(guide),
                    guide_link(guide)
                ));
            }
            if !note.is_empty() {
                note.push(' ');
            }
            note.push_str(&format!("Design: `docs/design/{}`.", api.design));
            out.push_str(&format!(
                "| {} | {} | {k} | {side} | {} | {note} |\n",
                cell(&api.name),
                status_word(api),
                verified(api),
            ));
        }
        let checks: Vec<String> = rows
            .iter()
            .flat_map(|a| {
                a.chromium
                    .iter()
                    .map(|t| format!("`chrome::{t}`"))
                    .chain(a.firefox.iter().map(|t| format!("`firefox::{t}`")))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !checks.is_empty() {
            out.push_str(&format!("\nReal-browser checks: {}.\n", checks.join(", ")));
        }
    }
    out
}

/// The page's generated part matches the data.
#[test]
fn web_platform_page_matches_the_data() {
    let path = mdbook::book_src().join(PAGE);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{PAGE} can't be read: {e}"));
    let start = text
        .find(GEN_START)
        .unwrap_or_else(|| panic!("{PAGE} has no `{GEN_START}` marker"));
    let body_start = start + GEN_START.len();
    let end = body_start
        + text[body_start..]
            .find(GEN_END)
            .unwrap_or_else(|| panic!("{PAGE} has no `{GEN_END}` after the start marker"));
    let generated = render(&data());
    if text[body_start..end] == generated {
        return;
    }
    if std::env::var_os("K_BLESS").is_some() {
        let updated = format!("{}{generated}{}", &text[..body_start], &text[end..]);
        fs::write(&path, updated).unwrap();
        return;
    }
    panic!(
        "{PAGE} doesn't match {DATA}; regenerate it with\n  \
         K_BLESS=1 cargo test -p k_sema --test it -- web_platform::"
    );
}

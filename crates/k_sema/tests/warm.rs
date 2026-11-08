//! Warm re-checks replay the body facts of unchanged files
//! (`docs/compiler/sema.md`, "Round 2 notes"). Each test applies edits to
//! a host and compares, after every edit, the warm analysis with a fresh
//! one: the diagnostics (with labels, notes and fixes), and the answers of
//! `type_at` and `resolve_at` at every node of every project file.
//!
//! The edits target the cases the replay must get right: a body edit that
//! shifts the positions another file's diagnostic points at, a body edit
//! that makes an import unused, a body error next to replayed files, and a
//! signature edit (which re-checks everything).

#![allow(clippy::unwrap_used, clippy::panic, missing_docs)]

#[path = "../../k_syntax/tests/support/kgen.rs"]
mod kgen;
#[path = "support/soracle.rs"]
mod soracle;

use std::fs;
use std::path::{Path, PathBuf};

use k_sema::{Analysis, AnalysisHost, Change, FileKind, Options, ProjectPath};
use k_syntax::SyntaxKind;

type Files = Vec<(String, String)>;

const MANIFEST: &str = "[package]\nname = \"corpus\"\nk = \"0.1\"\n";

/// Every project file's queries: `(path, start, end, type_at, resolve_at)`
/// for each node (trivia excluded), in a canonical order.
/// One answered query: `(path, start, end, type_at, resolve_at)`.
type Query = (String, u32, u32, Option<String>, Option<String>);

fn queries(analysis: &Analysis) -> Vec<Query> {
    let mut out = Vec::new();
    for (file, path) in analysis.files() {
        if !path.as_str().ends_with(".k") {
            continue;
        }
        let Some(parse) = analysis.parse(file).unwrap() else {
            continue;
        };
        for node in parse.syntax().descendants() {
            let tokens: Vec<_> = node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia())
                .collect();
            let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
                continue;
            };
            let range = rowan::TextRange::new(first.text_range().start(), last.text_range().end());
            let ty = analysis.type_at(file, range).unwrap();
            let res = analysis
                .resolve_at(file, range)
                .unwrap()
                .map(|r| format!("{r:?}"));
            if ty.is_some() || res.is_some() {
                out.push((
                    path.as_str().to_owned(),
                    u32::from(range.start()),
                    u32::from(range.end()),
                    ty,
                    res,
                ));
            }
        }
        // Name tokens carry resolutions of their own.
        for token in parse
            .syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::Ident)
        {
            let range = token.text_range();
            let res = analysis
                .resolve_at(file, range)
                .unwrap()
                .map(|r| format!("{r:?}"));
            let ty = analysis.type_at(file, range).unwrap();
            out.push((
                path.as_str().to_owned(),
                u32::from(range.start()),
                u32::from(range.end()),
                ty,
                res,
            ));
        }
    }
    out.sort();
    out
}

fn fresh(files: &Files) -> AnalysisHost {
    soracle::host(files, Options::new())
}

/// Compares `host` (warm) with a fresh analysis of `files`.
fn assert_same(host: &AnalysisHost, files: &Files, what: &str) {
    let warm = host.analysis();
    let cold_host = fresh(files);
    let cold = cold_host.analysis();
    let warm_diags = soracle::sorted(&soracle::check_analysis(&warm, files));
    let cold_diags = soracle::sorted(&soracle::check_analysis(&cold, files));
    assert_eq!(
        warm_diags, cold_diags,
        "{what}: warm diagnostics differ from a fresh check"
    );
    assert_eq!(
        warm.rpc_lock().unwrap(),
        cold.rpc_lock().unwrap(),
        "{what}: the warm rpc.lock differs from a fresh check's"
    );
    let (wp, cp) = (warm.program().unwrap(), cold.program().unwrap());
    let view =
        |p: &k_sema::view::ProgramView| format!("{:?}\n{:?}\n{:?}", p.decls, p.members, p.generics);
    assert!(
        view(wp) == view(cp),
        "{what}: the warm program view differs from a fresh check's"
    );
    let (wq, cq) = (queries(&warm), queries(&cold));
    if wq != cq {
        let first = wq
            .iter()
            .zip(cq.iter())
            .find(|(a, b)| a != b)
            .map(|(a, b)| format!("warm {a:?}\nfresh {b:?}"))
            .unwrap_or_else(|| format!("{} vs {} entries", wq.len(), cq.len()));
        panic!("{what}: warm queries differ from a fresh check\n{first}");
    }
}

/// Applies `edits` one by one (each a full new text for one path) and
/// compares the warm analysis with a fresh one after each.
fn run(mut files: Files, edits: &[(&str, String)]) {
    let mut host = fresh(&files);
    assert_same(&host, &files, "before the edits");
    for (step, (path, text)) in edits.iter().enumerate() {
        let mut change = Change::new();
        change.set_file(ProjectPath::new(path).unwrap(), text.as_str());
        host.apply_change(change);
        let slot = files.iter_mut().find(|(p, _)| p == path).unwrap();
        slot.1.clone_from(text);
        assert_same(&host, &files, &format!("after edit {step} of `{path}`"));
    }
}

fn file(path: &str, text: &str) -> (String, String) {
    (path.to_owned(), text.to_owned())
}

const SHAPE: &str = "func helper() -> Int {
    return 1
}

public interface Shape {
    func area() -> Double
}
";

const SQUARE: &str = "import m.a.Shape

public struct Square implements Shape {
    let side: Double

    func area() -> Int {
        return 1
    }
}
";

#[test]
fn a_body_edit_moves_the_label_of_another_files_diagnostic() {
    // `Square.area` doesn't match `Shape.area` (K2015, labelled at the
    // requirement in `A.k`). Growing `helper`'s body moves the requirement
    // down: `B.k`'s replayed diagnostic must follow.
    let files = vec![
        file("k.toml", MANIFEST),
        file("src/m/a/Shape.k", SHAPE),
        file("src/m/b/Square.k", SQUARE),
    ];
    let host = fresh(&files);
    let diags = soracle::check_analysis(&host.analysis(), &files);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "K2015" && d.labels.iter().any(|l| l.0 == "src/m/a/Shape.k")),
        "the case needs a K2015 labelled in `Shape.k`: {diags:#?}"
    );
    let grown = SHAPE.replace(
        "    return 1\n",
        "    let x = 1\n    let y = x\n    return y\n",
    );
    let back = SHAPE.to_owned();
    run(
        files,
        &[("src/m/a/Shape.k", grown), ("src/m/a/Shape.k", back)],
    );
}

#[test]
fn a_body_edit_can_make_an_import_unused_or_used() {
    let util = "public func twice(n: Int) -> Int {\n    return n * 2\n}\n";
    let user = "import m.util.twice\n\npublic func run() -> Int {\n    return twice(n: 2)\n}\n";
    let files = vec![
        file("k.toml", MANIFEST),
        file("src/m/util/twice.k", util),
        file("src/m/app/run.k", user),
    ];
    let unused = user.replace("return twice(n: 2)", "return 4");
    run(
        files,
        &[
            ("src/m/app/run.k", unused),
            ("src/m/app/run.k", user.to_owned()),
        ],
    );
}

#[test]
fn body_errors_and_replayed_files_mix() {
    let a = "public struct Item {\n    let price: Int\n}\n";
    let b = "import m.a.Item\n\npublic func total(items: List<Item>) -> Int {\n    var sum = 0\n    for item in items {\n        sum = sum + item.price\n    }\n    return sum\n}\n";
    let c = "import m.a.Item\n\npublic func first(items: List<Item>) -> Int? {\n    return items.first?.price\n}\n";
    let files = vec![
        file("k.toml", MANIFEST),
        file("src/m/a/Item.k", a),
        file("src/m/b/total.k", b),
        file("src/m/c/first.k", c),
    ];
    let broken = b.replace("item.price", "item.cost");
    let typo = c.replace("?.price", "?.prise");
    run(
        files,
        &[
            ("src/m/b/total.k", broken),
            ("src/m/c/first.k", typo),
            ("src/m/b/total.k", b.to_owned()),
        ],
    );
}

#[test]
fn a_signature_edit_re_checks_everything() {
    let a = "public struct Item {\n    let price: Int\n}\n";
    let b = "import m.a.Item\n\npublic func price(item: Item) -> Int {\n    return item.price\n}\n";
    let files = vec![
        file("k.toml", MANIFEST),
        file("src/m/a/Item.k", a),
        file("src/m/b/price.k", b),
    ];
    let renamed = a.replace("price", "cost");
    run(
        files,
        &[
            ("src/m/a/Item.k", renamed),
            ("src/m/a/Item.k", a.to_owned()),
        ],
    );
}

/// Regression (property `incremental_recheck_of_syntax_projects_equals_a_fresh_check`):
/// the type of an `@Inject` field of a `migration` (or `app`) is read by the
/// dependency-injection pass in every revision, so its diagnostics must not
/// be cached with the file's body facts; replaying them reported an unknown
/// type twice after an edit to another file.
#[test]
fn an_inject_field_of_a_migration_reports_once_after_a_replay() {
    let migration = "migration CartItem {\n    @Inject let name: Float\n}\n";
    let other = "public func one() -> Int {\n    return 1\n}\n";
    let files = vec![
        file("k.toml", MANIFEST),
        file("src/m/a/A.k", other),
        file("src/m/a/B.k", migration),
    ];
    let edited = other.replace("return 1", "return 2");
    run(
        files,
        &[
            ("src/m/a/A.k", other.to_owned()),
            ("src/m/a/A.k", edited),
            ("src/m/a/A.k", other.to_owned()),
        ],
    );
}

const CART_API: &str = "/// A helper above the protocol: growing its body moves the protocol.
func helper() -> Int {
    return 1
}

/// The cart, served to the browser.
public protocol CartApi {
    /// Adds an item; answers the new count.
    rpc add(item: Int) throws CartError -> String
}
";

const CART_SERVICE: &str = "/// The cart's implementation.
public service CartService implements CartApi {
    rpc add(item: Int) async throws CartError -> String {
        return \"{item}\"
    }
}
";

const CART_ERROR: &str = "import k.rpc.RpcFailure

/// Why a cart call failed.
public enum CartError implements Error, Codable {
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
";

/// An `rpc.lock` recording an rpc `clear` the protocol no longer has, so
/// the program breaks callers (K8001 at the protocol, labelled in the lock).
const CART_LOCK: &str = "[protocols.\"cart.CartApi\"]

[protocols.\"cart.CartApi\".rpcs.add]
params = [{ name = \"item\", type = \"Int\" }]
returns = \"String\"
throws = \"cart.CartError\"

[protocols.\"cart.CartApi\".rpcs.clear]
params = []
returns = \"Void\"
throws = \"cart.CartError\"

[types.\"cart.CartError\"]
cases = [\"rpcFailure\"]
";

#[test]
fn rpc_lock_facts_follow_body_edits_and_lock_edits() {
    // The `rpc.lock` check reads only signatures, the lock and the
    // manifests. A body edit that moves the protocol must move its K8001;
    // a body edit elsewhere, an edit of the lock and an edit of the manifest
    // must give what a fresh check gives, the lock it implies included.
    let files = vec![
        file("k.toml", "[package]\nname = \"shop\"\nk = \"0.1\"\n"),
        file("App.k", "public app Shop {}\n"),
        file("rpc.lock", CART_LOCK),
        file("src/cart/CartApi.k", CART_API),
        file("src/cart/CartService.k", CART_SERVICE),
        file("src/cart/CartError.k", CART_ERROR),
    ];
    let host = fresh(&files);
    let diags = soracle::check_analysis(&host.analysis(), &files);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "K8001" && d.path == "src/cart/CartApi.k"),
        "the case needs a K8001 at the protocol: {diags:#?}"
    );
    let grown = CART_API.replace(
        "    return 1\n",
        "    let x = 1\n    let y = x\n    return y\n",
    );
    let service = CART_SERVICE.replace("{item}", "item {item}");
    let compatible = CART_LOCK.replace(
        "[protocols.\"cart.CartApi\".rpcs.clear]\nparams = []\nreturns = \"Void\"\nthrows = \"cart.CartError\"\n\n",
        "",
    );
    run(
        files,
        &[
            ("src/cart/CartApi.k", grown),
            ("src/cart/CartService.k", service),
            ("src/cart/CartApi.k", CART_API.to_owned()),
            ("rpc.lock", compatible),
            ("src/cart/CartService.k", CART_SERVICE.to_owned()),
            ("rpc.lock", CART_LOCK.to_owned()),
            (
                "k.toml",
                "[package]\nname = \"store\"\nk = \"0.1\"\n".to_owned(),
            ),
            (
                "src/cart/CartService.k",
                CART_SERVICE.replace("{item}", "{item}!"),
            ),
        ],
    );
}

#[test]
fn page_facts_follow_body_edits() {
    // Unused classes (K3204), long descriptions (K3028) and a missing
    // title (K3027) are found by project passes over the declarations;
    // a body edit above them moves them.
    let long = "A".repeat(161);
    let pages = format!(
        "func helper() -> Int {{
    return 1
}}

page About at \"/about\" {{
    metadata {{
        description(\"{long}\")
    }}

    func count() -> Int {{
        return 2
    }}

    view {{
        text {{ \"About\" }}
    }}

    style {{
        .unused {{
            color: red
        }}
    }}
}}

page Product at \"/product\" {{
    @Load let name: String = try await fetchName()

    view {{
        text {{ name }}
    }}

    failed(let error) {{
        metadata {{
            description(\"{long}\")
        }}

        text {{ \"Missing\" }}
    }}
}}

func fetchName() async throws LoadError -> String {{
    return \"x\"
}}
"
    );
    let files = vec![
        file("k.toml", MANIFEST),
        file("App.k", "public app Site {}\n"),
        file("src/site/Pages.k", &pages),
    ];
    let host = fresh(&files);
    let codes: Vec<String> = soracle::check_analysis(&host.analysis(), &files)
        .into_iter()
        .map(|d| d.code)
        .collect();
    for code in ["K3027", "K3028", "K3204"] {
        assert!(
            codes.iter().any(|c| c == code),
            "the case needs {code}: {codes:?}"
        );
    }
    let grown = pages.replace(
        "    return 1\n",
        "    let x = 1\n    let y = x\n    return y\n",
    );
    let inner = pages.replace(
        "        return 2\n",
        "        let z = 2\n        return z\n",
    );
    run(
        files,
        &[
            ("src/site/Pages.k", grown),
            ("src/site/Pages.k", inner),
            ("src/site/Pages.k", pages.clone()),
        ],
    );
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn collect(dir: &Path, root: &Path, out: &mut Files) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "dist" && name != "target" {
                collect(&path, root, out);
            }
            continue;
        }
        let relative: Vec<String> = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let relative = relative.join("/");
        if ProjectPath::new(&relative).unwrap().kind() != FileKind::Other {
            out.push((relative, fs::read_to_string(&path).unwrap()));
        }
    }
}

/// In every example, each file's first function body gets a statement
/// that changes its diagnostics (an unused binding, K2304… or an
/// unresolved name), one file after another, then the file is restored.
#[test]
fn body_edits_across_the_examples_match_fresh_checks() {
    let mut found: Vec<PathBuf> = fs::read_dir(examples_dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("k.toml").is_file())
        .collect();
    found.sort();
    for example in found {
        let mut files = Files::new();
        collect(&example, &example, &mut files);
        files.sort();
        let mut edits: Vec<(String, String)> = Vec::new();
        for (path, text) in &files {
            if !path.ends_with(".k") {
                continue;
            }
            // The first `func … {` line whose body starts on the next line.
            let Some(at) = text
                .match_indices("func ")
                .find_map(|(i, _)| text[i..].find("{\n").map(|j| i + j + 2))
            else {
                continue;
            };
            let edited = format!(
                "{}        let probe = unknownName\n{}",
                &text[..at],
                &text[at..]
            );
            edits.push((path.clone(), edited));
            edits.push((path.clone(), text.clone()));
            if edits.len() >= 24 {
                break;
            }
        }
        let edits: Vec<(&str, String)> =
            edits.iter().map(|(p, t)| (p.as_str(), t.clone())).collect();
        run(files, &edits);
    }
}

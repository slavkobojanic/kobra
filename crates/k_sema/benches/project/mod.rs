//! Generated K projects for the semantic-analysis benchmarks and budget
//! checks.
//!
//! Everything here is a pure function of its arguments (sizes and a seed),
//! so every run measures byte-identical projects. The output is shared by
//! `benches/sema.rs`, `tests/perf_budget.rs` and `tests/project_workload.rs`
//! through `#[path]` includes; the last one is part of the normal test suite
//! and guarantees that the realistic project and every pathological family
//! analyse with **zero diagnostics**, so the benchmarks never measure error
//! paths by accident.
//!
//! # Realistic projects
//!
//! [`realistic`] builds an app (`k.toml`, `App.k`, a theme, a shared `core`
//! module of generic types) and then feature modules until the requested
//! number of lines is reached. Every feature module has the files a real
//! feature has:
//!
//! | File | Declaration |
//! |---|---|
//! | `Record<i>.k` | a `Codable` struct with fields, a computed property and methods |
//! | `Status<i>.k` | an enum with payload cases and exhaustive `match`es |
//! | `Failure<i>.k` | an error enum |
//! | `Shape<i>.k` | an interface with a default method |
//! | `Protocol<i>.k` | a protocol with typed-`throws` rpcs |
//! | `Api<i>.k` | the server service implementing the protocol |
//! | `Service<i>.k` | a client service injecting the protocol and the previous module's service, with state, derived values and generic helpers |
//! | `Card<i>.k` | a component with props, state, a view and a style block |
//! | `Screen<i>.k` | a page with `@Inject`, `@Load`, `loading`, `failed`, a view and styles |
//! | `Service<i>.test.k` | a test suite |
//!
//! Modules import their predecessor (and `core`), so the module graph is a
//! long chain with fan-in on `core`, which is what real layered apps look
//! like. The last module gets top-level helper functions so the line count
//! is exactly the one requested.
//!
//! # Pathological projects
//!
//! [`Pathological`] families each stress one dimension of the checker; see
//! its variants.

#![allow(dead_code)]

use std::fmt::Write as _;

/// The seed every benchmark uses.
pub const SEED: u64 = 0x5EED_0F4B_5E4A;

/// One project file: its project-relative path and its text.
pub type File = (String, String);

/// A small, fast, seedable generator (SplitMix64).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Creates a generator from `seed`.
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// Returns the next 64 random bits.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a number in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Returns a number in `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    /// Picks one element of `items`.
    pub fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// Counts the lines of `text` (a final line without a newline counts).
pub fn line_count(text: &str) -> usize {
    let newlines = text.bytes().filter(|&b| b == b'\n').count();
    newlines + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

/// Counts the lines of every `.k` file of a project.
pub fn project_lines(files: &[File]) -> usize {
    files
        .iter()
        .filter(|(p, _)| p.ends_with(".k"))
        .map(|(_, t)| line_count(t))
        .sum()
}

/// Counts the bytes of every `.k` file of a project.
pub fn project_bytes(files: &[File]) -> usize {
    files
        .iter()
        .filter(|(p, _)| p.ends_with(".k"))
        .map(|(_, t)| t.len())
        .sum()
}

/// The `k.toml` of every generated project.
const MANIFEST: &str =
    "[package]\nname = \"bench\"\ntitle = \"Bench\"\nk = \"0.1\"\nversion = \"0.1.0\"\n";

/// `App.k`: the app declaration with its metadata.
const APP: &str = "\
/// The benchmark app: every page is found from its declaration.
public app Bench {
    metadata {
        manifest(name: \"Bench\", shortName: \"Bench\")
        themeColor(.primary)
        locale(.english)
    }
}
";

/// The app's theme: the tokens the generated styles use, with a dark
/// variant.
const THEME: &str = "\
/// Design tokens shared by every component of the benchmark app.
public theme BenchTheme {
    color primary: Color = #2563EB
    color onPrimary: Color = #FFFFFF
    color background: Color = #F1F5F9
    color surface: Color = #FFFFFF
    color text: Color = #0F172A
    color muted: Color = #64748B
    color border: Color = #E2E8F0
    color danger: Color = #B91C1C

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius sm: Radius = 4px
    radius md: Radius = 8px

    fontSize sm: FontSize = 0.875rem
    fontSize xl: FontSize = 1.75rem

    variant dark {
        primary = #60A5FA
        background = #020617
        surface = #0F172A
        text = #F1F5F9
        muted = #94A3B8
        border = #334155
    }
}
";

/// `core/Pair.k`: a generic struct with two type parameters and generic
/// methods.
const CORE_PAIR: &str = "\
/// Two values of possibly different types.
public struct Pair<A, B> {
    /// The first value.
    let first: A

    /// The second value.
    let second: B

    /// Returns the pair with its values swapped.
    func swapped() -> Pair<B, A> {
        return Pair(first: second, second: first)
    }

    /// Returns a pair whose first value is `transform(first)`.
    func mapFirst<C>(transform: (A) -> C) -> Pair<C, B> {
        return Pair(first: transform(first), second: second)
    }
}
";

/// `core/Batch.k`: a generic struct with a computed property and methods
/// taking lambdas.
const CORE_BATCH: &str = "\
/// A page of results with the total count, as list screens show them.
public struct Batch<T> {
    /// The items of this page.
    let items: List<T>

    /// How many items exist in total.
    let total: Int

    /// Whether more pages follow this one.
    var hasMore: Bool {
        return items.count < total
    }

    /// Returns the batch with `transform` applied to every item.
    func map<U>(transform: (T) -> U) -> Batch<U> {
        return Batch(items: items.map(transform), total: total)
    }

    /// Returns the first item for which `test` is `true`.
    func find(test: (T) -> Bool) -> T? {
        return items.firstWhere(test)
    }
}
";

/// `core/Outcome.k`: a generic enum with payload cases.
const CORE_RESULT: &str = "\
/// The outcome of a validation step.
public enum Outcome<T> {
    /// The step succeeded with a value.
    case accepted(value: T)

    /// The step failed with a reason.
    case rejected(reason: String)

    /// Whether the step succeeded.
    var isAccepted: Bool {
        return match self {
            case .accepted { true }
            case .rejected { false }
        }
    }
}
";

/// `core/Labeled.k`: an interface with a default method.
const CORE_LABELED: &str = "\
/// Something with a human-readable label.
public interface Labeled {
    /// The label shown in lists.
    func label() -> String

    /// The label in upper case.
    func shout() -> String {
        return label().uppercased()
    }
}
";

/// `core/largest.k`: a generic top-level function taking a lambda.
const CORE_UTIL: &str = "\
/// Returns the largest of `items` by `key`, or `nil` for an empty list.
public func largest<T>(items: List<T>, key: (T) -> Int) -> T? {
    var best: T? = nil
    var bestKey = 0
    for item in items {
        let k = key(item)
        if best == nil or k > bestKey {
            best = item
            bestKey = k
        }
    }
    return best
}
";

/// The realistic project of about `lines` lines of K (exactly `lines` when
/// `lines` is at least the size of the fixed files, about 160).
pub fn realistic(lines: usize, seed: u64) -> Vec<File> {
    let mut rng = Rng::new(seed);
    let mut files: Vec<File> = vec![
        ("k.toml".into(), MANIFEST.into()),
        ("App.k".into(), APP.into()),
        ("src/ui/BenchTheme.k".into(), THEME.into()),
        ("src/core/Pair.k".into(), CORE_PAIR.into()),
        ("src/core/Batch.k".into(), CORE_BATCH.into()),
        ("src/core/Outcome.k".into(), CORE_RESULT.into()),
        ("src/core/Labeled.k".into(), CORE_LABELED.into()),
        ("src/core/largest.k".into(), CORE_UTIL.into()),
    ];
    let mut total = project_lines(&files);
    let mut index = 0;
    loop {
        let module = feature_module(index, &mut rng);
        let size = project_lines(&module);
        // Keep room for the filler's fixed lines (its doc and signature).
        if index > 0 && total + size + 8 > lines {
            break;
        }
        total += size;
        files.extend(module);
        index += 1;
    }
    let rest = lines.saturating_sub(total);
    if rest > 0 {
        files.push(filler(index.saturating_sub(1), rest));
    }
    files
}

/// The module name of feature module `i`.
pub fn module_name(i: usize) -> String {
    format!("feature{i:04}")
}

/// The path of the file that [`body_edit`] and [`signature_edit`] change in
/// a realistic project (feature module 0's service).
pub fn edited_service_path() -> String {
    format!("src/{}/Service0.k", module_name(0))
}

/// Returns `text` with one function body changed (a literal in the body of
/// a private helper), keeping the project free of diagnostics. Applied to
/// the file at [`edited_service_path`], it is the "warm body edit".
pub fn body_edit(text: &str, generation: usize) -> String {
    let marker = "return value * 3 + ";
    let start = text.find(marker).map(|i| i + marker.len());
    match start {
        Some(start) => {
            let end = start + text[start..].find('\n').unwrap_or(0);
            format!("{}{}{}", &text[..start], generation + 2, &text[end..])
        }
        None => text.to_owned(),
    }
}

/// Returns `text` with a public signature changed: a new public method is
/// added to the service (its member set and so its signature change),
/// keeping the project free of diagnostics.
pub fn signature_edit(text: &str, generation: usize) -> String {
    let end = text.rfind('}').unwrap_or(text.len());
    format!(
        "{}\n    /// Added by the signature-edit benchmark.\n    func added{generation}(value: Int) -> Int {{\n        return value + {generation}\n    }}\n{}",
        &text[..end],
        &text[end..]
    )
}

/// Returns `Helpers.k` of feature module `module`: private four-line
/// helper functions, padded with blank lines to exactly `lines` lines.
fn filler(module: usize, lines: usize) -> File {
    let mut text = String::new();
    // Each helper is 4 lines; the header and leftovers are blank lines.
    let helpers = lines / 4;
    let mut written = 0;
    for h in 0..helpers {
        let _ = write!(
            text,
            "/// Helper {h}.\npublic func helper{h}(x: Int) -> Int {{\n    return x * {} + {h}\n}}\n",
            h % 7 + 2
        );
        written += 4;
    }
    while written < lines {
        text.push('\n');
        written += 1;
    }
    (
        format!("src/{}/Helpers.k", module_name(module)),
        text.replace("public func", "func"),
    )
}

/// Names of the `String` fields of generated records.
const FIELD_NAMES: &[&str] = &[
    "title", "summary", "owner", "headline", "slug", "notes", "code", "region", "author",
    "category",
];
/// Names of the `Int` fields of generated records.
const INT_NAMES: &[&str] = &[
    "count", "rank", "quantity", "score", "weight", "position", "priority", "size", "age", "level",
];

/// The files of feature module `i`.
fn feature_module(i: usize, rng: &mut Rng) -> Vec<File> {
    let m = module_name(i);
    let dir = format!("src/{m}");
    let strings: Vec<&str> = (0..rng.range(3, 5))
        .map(|k| FIELD_NAMES[(i + k * 3) % FIELD_NAMES.len()])
        .collect();
    let ints: Vec<&str> = (0..rng.range(3, 5))
        .map(|k| INT_NAMES[(i * 7 + k * 3) % INT_NAMES.len()])
        .collect();
    let cases = rng.range(4, 10);
    let methods = rng.range(10, 24);
    let rpcs = rng.range(2, 6);
    let mut files = vec![
        (format!("{dir}/Record{i}.k"), record(i, &strings, &ints)),
        (format!("{dir}/Status{i}.k"), status(i, cases)),
        (format!("{dir}/Failure{i}.k"), failure(i)),
        (format!("{dir}/Shape{i}.k"), shape(i)),
        (format!("{dir}/Protocol{i}.k"), protocol(i, &ints, rpcs)),
        (format!("{dir}/Api{i}.k"), api(i, &strings, &ints, rpcs)),
        (format!("{dir}/Service{i}.k"), service(i, &ints, methods)),
        (format!("{dir}/Card{i}.k"), card(i, &strings, &ints)),
        (format!("{dir}/Screen{i}.k"), screen(i, &ints)),
        (format!("{dir}/Service{i}.test.k"), test_suite(i, methods)),
    ];
    files.retain(|(_, t)| !t.is_empty());
    files
}

/// `Record<i>.k`: a `Codable`, `Equatable` struct with the given fields,
/// implementing `Labeled` and `Shape<i>`.
fn record(i: usize, strings: &[&str], ints: &[&str]) -> String {
    let mut t = String::from("import core.Labeled\n\n");
    let _ = writeln!(t, "/// A stored record of feature {i}.");
    let _ = writeln!(
        t,
        "public struct Record{i} implements Codable, Equatable, Labeled, Shape{i} {{"
    );
    let _ = writeln!(t, "    /// Identifies the record.\n    let id: Int\n");
    for s in strings {
        let _ = writeln!(t, "    /// The record's {s}.\n    var {s}: String\n");
    }
    for n in ints {
        let _ = writeln!(t, "    /// The record's {n}.\n    var {n}: Int\n");
    }
    let _ = writeln!(
        t,
        "    /// The state of the record.\n    var status: Status{i}\n"
    );
    let _ = writeln!(t, "    /// The sum of the record's numbers.");
    let sum = ints.join(" + ");
    let _ = writeln!(t, "    var total: Int {{\n        return {sum}\n    }}\n");
    let _ = writeln!(t, "    /// The record's size, for `Shape{i}`.");
    let _ = writeln!(
        t,
        "    func measure() -> Int {{\n        return total * 2\n    }}\n"
    );
    let _ = writeln!(t, "    /// The label shown in lists.");
    let _ = writeln!(
        t,
        "    func label() -> String {{\n        return \"{{{}}} ({{total}})\"\n    }}\n",
        strings[0]
    );
    let _ = writeln!(
        t,
        "    /// Whether the record's text contains `query`, ignoring case."
    );
    let _ = writeln!(t, "    func matches(query: String) -> Bool {{");
    let _ = writeln!(t, "        let needle = query.lowercased()");
    let conds: Vec<String> = strings
        .iter()
        .map(|s| format!("{s}.lowercased().contains(needle)"))
        .collect();
    let _ = writeln!(t, "        return {}", conds.join(" or "));
    let _ = writeln!(t, "    }}\n");
    let _ = writeln!(
        t,
        "    /// Returns a copy with `{}` raised by `amount`.",
        ints[0]
    );
    let _ = writeln!(t, "    func raised(amount: Int) -> Record{i} {{");
    let _ = writeln!(
        t,
        "        var copy = self\n        copy.{} = {} + amount\n        return copy\n    }}",
        ints[0], ints[0]
    );
    t.push_str("}\n");
    t
}

/// `Status<i>.k`: an enum with `cases` cases (some with payloads) and
/// members that `match` over them exhaustively.
fn status(i: usize, cases: usize) -> String {
    let mut t = String::new();
    let _ = writeln!(t, "/// Where a record of feature {i} is in its life cycle.");
    let _ = writeln!(t, "public enum Status{i} implements Codable, Equatable {{");
    let _ = writeln!(t, "    /// Not yet started.\n    case draft\n");
    let _ = writeln!(
        t,
        "    /// In progress, with a completion percentage.\n    case active(percent: Int)\n"
    );
    let _ = writeln!(
        t,
        "    /// Waiting for someone, with a reason.\n    case blocked(reason: String)\n"
    );
    for c in 3..cases {
        let _ = writeln!(t, "    /// Stage {c}.\n    case stage{c}\n");
    }
    let _ = writeln!(t, "    /// Finished.\n    case done\n");
    let _ = writeln!(t, "    /// A short description for badges.");
    let _ = writeln!(
        t,
        "    var caption: String {{\n        return match self {{"
    );
    let _ = writeln!(t, "            case .draft {{ \"Draft\" }}");
    let _ = writeln!(
        t,
        "            case .active(let percent) where percent >= 50 {{ \"Almost there: {{percent}}%\" }}"
    );
    let _ = writeln!(
        t,
        "            case .active(let percent) {{ \"Active: {{percent}}%\" }}"
    );
    let _ = writeln!(
        t,
        "            case .blocked(let reason) {{ \"Blocked: {{reason}}\" }}"
    );
    for c in 3..cases {
        let _ = writeln!(t, "            case .stage{c} {{ \"Stage {c}\" }}");
    }
    let _ = writeln!(t, "            case .done {{ \"Done\" }}");
    let _ = writeln!(t, "        }}\n    }}\n");
    let _ = writeln!(t, "    /// Whether no more work is expected.");
    let _ = writeln!(
        t,
        "    func isSettled() -> Bool {{\n        return match self {{"
    );
    let _ = writeln!(t, "            case .done {{ true }}");
    let _ = writeln!(t, "            default {{ false }}");
    let _ = writeln!(t, "        }}\n    }}");
    t.push_str("}\n");
    t
}

/// `Failure<i>.k`: the error enum of feature `i`.
fn failure(i: usize) -> String {
    format!(
        "import k.rpc.RpcFailure

/// Why an operation of feature {i} failed.
public enum Failure{i} implements Error, Codable {{
    /// The server couldn't be reached.
    case rpcFailure(RpcFailure)

    /// The record does not exist.
    case notFound

    /// The input was rejected.
    case invalid(reason: String)

    /// A sentence for the user.
    var message: String {{
        return match self {{
            case .rpcFailure {{ \"The server couldn't be reached.\" }}
            case .notFound {{ \"Nothing was found.\" }}
            case .invalid(let reason) {{ \"That didn't work: {{reason}}\" }}
        }}
    }}
}}
"
    )
}

/// `Shape<i>.k`: an interface with a requirement and a default method.
fn shape(i: usize) -> String {
    format!(
        "/// Something feature {i} can measure.
public interface Shape{i} {{
    /// The size of the shape.
    func measure() -> Int

    /// Twice the size.
    func doubled() -> Int {{
        return measure() * 2
    }}
}}
"
    )
}

/// `Protocol<i>.k`: a protocol with typed-`throws` rpcs, `rpcs` of them
/// counting records by an `Int` field.
fn protocol(i: usize, ints: &[&str], rpcs: usize) -> String {
    let mut extra = String::new();
    for k in 0..rpcs {
        let n = ints[k % ints.len()];
        let _ = write!(
            extra,
            "\n    /// Counts the records whose `{n}` is at least `minimum`.\n    ///\n    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.\n    rpc count{k}(minimum: Int) throws RpcOnlyError -> Int\n"
        );
    }
    format!(
        "import k.rpc.RpcOnlyError

/// The contract between feature {i}'s screens and the server.
public protocol Protocol{i} {{
    /// Lists every record.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    rpc list() throws RpcOnlyError -> List<Record{i}>

    /// Finds one record.
    ///
    /// - Throws: `Failure{i}.notFound` when there is none.
    rpc find(id: Int) throws Failure{i} -> Record{i}

    /// Saves a record.
    ///
    /// - Throws: `Failure{i}.invalid` when the record is rejected.
    rpc save(record: Record{i}) throws Failure{i} -> Record{i}
{extra}}}
"
    )
}

/// `Api<i>.k`: the server service implementing `Protocol<i>`.
fn api(i: usize, strings: &[&str], ints: &[&str], rpcs: usize) -> String {
    let mut extra = String::new();
    for k in 0..rpcs {
        let n = ints[k % ints.len()];
        let _ = write!(
            extra,
            "\n    rpc count{k}(minimum: Int) async throws RpcOnlyError -> Int {{\n        return rows.count(where: {{ r => r.{n} >= minimum }})\n    }}\n"
        );
    }
    let mut fields = vec!["id: id".to_owned()];
    for s in strings {
        fields.push(format!("{s}: \"{s} {{id}}\""));
    }
    for (k, n) in ints.iter().enumerate() {
        fields.push(format!("{n}: id * {}", k + 2));
    }
    fields.push("status: .draft".into());
    format!(
        "import k.rpc.RpcOnlyError

/// The server implementation of `Protocol{i}`, over an in-memory table.
public service Api{i} implements Protocol{i} {{
    @State var rows: List<Record{i}> = []

    rpc list() async throws RpcOnlyError -> List<Record{i}> {{
        return rows
    }}

    rpc find(id: Int) async throws Failure{i} -> Record{i} {{
        let found = rows.firstWhere {{ r => r.id == id }}
        guard let row = found else {{
            throw Failure{i}.notFound
        }}
        return row
    }}

    rpc save(record: Record{i}) async throws Failure{i} -> Record{i} {{
        guard record.{s0}.isNotEmpty else {{
            throw Failure{i}.invalid(reason: \"{s0} is empty\")
        }}
        rows.append(record)
        return record
    }}

    /// A record made from an id, for seeding.
    func sample(id: Int) -> Record{i} {{
        return Record{i}({fields})
    }}
{extra}}}
",
        s0 = strings[0],
        fields = fields.join(", "),
        extra = extra,
    )
}

/// `Service<i>.k`: the client service of feature `i`, injecting the
/// protocol and the previous feature's service, with state, derived
/// values and `methods` generic helpers.
fn service(i: usize, ints: &[&str], methods: usize) -> String {
    let n0 = ints[0];
    let mut t = String::from(
        "import core.Batch\nimport core.Pair\nimport core.largest\nimport k.rpc.RpcOnlyError\n",
    );
    if i > 0 {
        let _ = writeln!(t, "import {}.Service{}", module_name(i - 1), i - 1);
    }
    t.push('\n');
    let _ = writeln!(
        t,
        "/// The client-side state of feature {i}: the loaded records and a selection."
    );
    let _ = writeln!(t, "public service Service{i} {{");
    let _ = writeln!(t, "    @Inject let api: Protocol{i}");
    if i > 0 {
        let _ = writeln!(t, "    @Inject let previous: Service{}", i - 1);
    }
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "    /// The loaded records.\n    @State var records: List<Record{i}> = []\n"
    );
    let _ = writeln!(
        t,
        "    /// The selected record's id, if any.\n    @State var selected: Int? = nil\n"
    );
    let _ = writeln!(
        t,
        "    /// How many records are loaded.\n    @Derived let count: Int = records.count\n"
    );
    let _ = writeln!(t, "    /// How many records have a positive `{n0}`.");
    let _ = writeln!(
        t,
        "    @Derived let total{n0}: Int = records.count(where: {{ r => r.{n0} > 0 }})\n"
    );
    let _ = writeln!(
        t,
        "    /// Loads every record.\n    ///\n    /// - Throws: `RpcOnlyError.rpcFailure` when offline."
    );
    let _ = writeln!(t, "    func load() async throws RpcOnlyError {{");
    let _ = writeln!(t, "        records = try await api.list()");
    let _ = writeln!(t, "    }}\n");
    let _ = writeln!(
        t,
        "    /// Fetches every record without keeping them.\n    ///\n    /// - Throws: `RpcOnlyError.rpcFailure` when offline."
    );
    let _ = writeln!(
        t,
        "    func fetch() async throws RpcOnlyError -> List<Record{i}> {{"
    );
    let _ = writeln!(t, "        return try await api.list()");
    let _ = writeln!(t, "    }}\n");
    let _ = writeln!(
        t,
        "    /// The records whose text matches `query`, as a batch."
    );
    let _ = writeln!(t, "    func search(query: String) -> Batch<Record{i}> {{");
    let _ = writeln!(
        t,
        "        let found = records.filter {{ r => r.matches(query: query) }}"
    );
    let _ = writeln!(
        t,
        "        return Batch(items: found, total: records.count)"
    );
    let _ = writeln!(t, "    }}\n");
    let _ = writeln!(
        t,
        "    /// The record with the largest `{n0}`, paired with its label."
    );
    let _ = writeln!(t, "    func best() -> Pair<Int, String>? {{");
    let _ = writeln!(
        t,
        "        guard let top = largest(items: records, key: {{ r => r.{n0} }}) else {{"
    );
    let _ = writeln!(t, "            return nil\n        }}");
    let _ = writeln!(t, "        return Pair(first: top.id, second: top.label())");
    let _ = writeln!(t, "    }}\n");
    let prev = if i > 0 { " + previous.count" } else { "" };
    for k in 0..methods {
        let n = ints[k % ints.len()];
        let _ = writeln!(
            t,
            "    /// Summary {k}: a number derived from every loaded record's `{n}`, for the"
        );
        let _ = writeln!(
            t,
            "    /// overview. Records at or below `limit` count against the result."
        );
        let _ = writeln!(t, "    func summary{k}(limit: Int) -> Int {{");
        match k % 4 {
            0 => {
                let _ = writeln!(t, "        var accumulated = 0");
                let _ = writeln!(t, "        for record in records {{");
                let _ = writeln!(
                    t,
                    "            if record.{n} > limit {{\n                accumulated = accumulated + scaled(value: record.{n})"
                );
                let _ = writeln!(
                    t,
                    "            }} else {{\n                accumulated = accumulated - {k}\n            }}"
                );
                let _ = writeln!(t, "        }}");
                let _ = writeln!(t, "        return accumulated{prev}");
            }
            1 => {
                let _ = writeln!(
                    t,
                    "        let above = records.filter {{ record => record.{n} > limit }}"
                );
                let _ = writeln!(
                    t,
                    "        let weights = above.map {{ record => scaled(value: record.{n}) }}"
                );
                let _ = writeln!(
                    t,
                    "        let ranked = above.sortedBy {{ record => record.{n} }}"
                );
                let _ = writeln!(
                    t,
                    "        guard let first = ranked.first else {{\n            return {k}{prev}\n        }}"
                );
                let _ = writeln!(t, "        return weights.count + first.{n}{prev}");
            }
            2 => {
                let _ = writeln!(t, "        var settled = 0");
                let _ = writeln!(t, "        for record in records {{");
                let _ = writeln!(
                    t,
                    "            guard record.{n} >= limit else {{\n                continue\n            }}"
                );
                let _ = writeln!(t, "            let weight = match record.status {{");
                let _ = writeln!(t, "                case .done {{ 2 }}");
                let _ = writeln!(
                    t,
                    "                case .active(let percent) {{ percent / 50 }}"
                );
                let _ = writeln!(t, "                default {{ 0 }}");
                let _ = writeln!(t, "            }}");
                let _ = writeln!(t, "            settled = settled + weight");
                let _ = writeln!(t, "        }}");
                let _ = writeln!(t, "        return settled{prev}");
            }
            _ => {
                let _ = writeln!(t, "        let chosen = selected ?? limit");
                let _ = writeln!(
                    t,
                    "        let candidate = records.firstWhere {{ record => record.id == chosen }}"
                );
                let _ = writeln!(
                    t,
                    "        if let found = candidate {{\n            return scaled(value: found.{n} + limit){prev}\n        }}"
                );
                let _ = writeln!(
                    t,
                    "        return records.count(where: {{ record => record.{n} < limit }}){prev}"
                );
            }
        }
        let _ = writeln!(t, "    }}\n");
    }
    let _ = writeln!(t, "    /// Selects the record with `id`.");
    let _ = writeln!(
        t,
        "    func select(id: Int) {{\n        selected = id\n    }}\n"
    );
    let _ = writeln!(t, "    /// Scales a number for summaries.");
    let _ = writeln!(t, "    private func scaled(value: Int) -> Int {{");
    let _ = writeln!(t, "        return value * 3 + 1");
    let _ = writeln!(t, "    }}");
    t.push_str("}\n");
    t
}

/// `Card<i>.k`: a component showing one record, with props, state, a view
/// and a style block.
fn card(i: usize, strings: &[&str], ints: &[&str]) -> String {
    let mut details = String::new();
    for s in strings {
        let _ = writeln!(
            details,
            "                paragraph {{ \"{s}: {{record.{s}}}\" }}\n                    .class(.detail)"
        );
    }
    for n in ints {
        let _ = writeln!(
            details,
            "                paragraph {{ \"{n}: {{record.{n}}}\" }}\n                    .class(.detail)"
        );
    }
    format!(
        "/// One record of feature {i} as a card that can be expanded.
public component Card{i} {{
    /// The record to show.
    @Prop let record: Record{i}

    @State var isExpanded: Bool = false

    @Derived let heading: String = record.label()

    func toggle() {{
        isExpanded = not isExpanded
    }}

    view {{
        box {{
            heading(level: 2) {{ heading }}
            paragraph {{ record.status.caption }}
                .class(.caption)

            match record.status {{
                case .done {{
                    paragraph {{ \"Finished\" }}
                        .class(.badge)
                }}
                case .blocked(let reason) {{
                    paragraph {{ reason }}
                        .class([.badge, .warning])
                }}
                default {{
                    paragraph {{ \"{{record.total}} in total\" }}
                }}
            }}

            if isExpanded {{
{details}            }}

            button {{ \"Toggle\" }}
                .on(click: toggle)
        }}
        .class([.card, .open when isExpanded])
    }}

    style {{
        .card {{
            display: flex;
            flexDirection: column;
            gap: .sm;
            padding: .md;
            borderRadius: .md;
            border: 1px solid .border;
            backgroundColor: .surface;
        }}

        .open {{ borderColor: .primary; }}
        .caption {{ color: .muted; fontSize: .sm; }}
        .detail {{ margin: 0; color: .text; }}
        .badge {{
            padding: .xs;
            borderRadius: .sm;
            backgroundColor: .background;
        }}
        .warning {{ color: .danger; }}

        @media (maxWidth: 40rem) {{
            .card {{ padding: .sm; }}
        }}
    }}
}}
"
    )
}

/// `Screen<i>.k`: a page with `@Inject`, `@Load`, `loading`, `failed`, a
/// view and styles.
fn screen(i: usize, ints: &[&str]) -> String {
    let n0 = ints[0];
    format!(
        "/// Every record of feature {i}.
public page Screen{i} {{
    @Inject let service: Service{i}

    /// The records, loaded before the page renders.
    @Load let records: List<Record{i}> = try await service.fetch()

    @State var query: String = \"\"

    @Derived let shown: List<Record{i}> = records.filter {{ r => r.matches(query: query) }}

    metadata {{
        title(\"Feature {i}\")
    }}

    view {{
        heading(level: 1) {{ \"Feature {i}\" }}

        textField(value: bind query)
            .label(\"Search\")

        if shown.isEmpty {{
            paragraph {{ \"Nothing matches.\" }}
                .class(.empty)
        }} else {{
            box {{
                for record in shown key record.id {{
                    Card{i}(record: record)
                }}
            }}
            .class(.grid)
        }}

        paragraph {{ \"Total {n0}: {{service.total{n0}}}\" }}
    }}

    loading {{
        paragraph {{ \"Loading…\" }}
    }}

    failed(let error) {{
        match error {{
            case .rpcFailure {{
                paragraph {{ \"The records couldn't be loaded.\" }}
                button {{ \"Try again\" }}
                    .on(click: reload)
            }}
        }}
    }}

    style {{
        .grid {{
            display: grid;
            gap: .lg;
        }}

        .empty {{ color: .muted; }}
    }}
}}
"
    )
}

/// `Service<i>.test.k`: a test suite for `Service<i>` with at most eight
/// generated cases.
fn test_suite(i: usize, methods: usize) -> String {
    let mut extra = String::new();
    for k in 0..methods.min(8) {
        let _ = write!(
            extra,
            "\n    test \"summary {k} of an empty service is its base\" {{\n        let value = service.summary{k}(limit: {k})\n\n        expect value >= 0\n    }}\n"
        );
    }
    format!(
        "test Service{i} {{
    @Inject let service: Service{i}

    test \"an empty service has no records\" {{
        expect service.count == 0
        expect service.best() == nil
    }}

    test \"searching an empty service finds nothing\" {{
        let batch = service.search(query: \"x\")

        expect batch.items.isEmpty
        expect not batch.hasMore
    }}
{extra}}}
"
    )
}

/// A pathological project family: each stresses one dimension of the
/// checker. Every family analyses without diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pathological {
    /// Generic types nested `size` deep (`Wrap<Wrap<...<Int>>>`), in
    /// annotations and in constructor calls that must be inferred, repeated
    /// over many functions.
    DeepGenerics,
    /// An enum with `size` cases and functions that `match` over all of
    /// them exhaustively, with and without payloads.
    HugeMatch,
    /// `size` modules, each importing the previous one's struct and
    /// service: the longest import chain and DI chain.
    ImportChain,
    /// One service injecting `size` services, each with an injected
    /// dependency of its own, and a page injecting the root: the widest DI
    /// graph.
    WideDi,
}

impl Pathological {
    /// Every family.
    pub const ALL: [Pathological; 4] = [
        Pathological::DeepGenerics,
        Pathological::HugeMatch,
        Pathological::ImportChain,
        Pathological::WideDi,
    ];

    /// The family's name in benchmark ids.
    pub fn name(self) -> &'static str {
        match self {
            Pathological::DeepGenerics => "deep-generics",
            Pathological::HugeMatch => "huge-match",
            Pathological::ImportChain => "import-chain",
            Pathological::WideDi => "wide-di",
        }
    }

    /// The size used by the benchmarks and the budget check.
    pub fn default_size(self) -> usize {
        match self {
            Pathological::DeepGenerics => 48,
            Pathological::HugeMatch => 2_000,
            Pathological::ImportChain => 1_000,
            Pathological::WideDi => 1_000,
        }
    }

    /// Generates the family at `size`.
    pub fn generate(self, size: usize) -> Vec<File> {
        let mut files: Vec<File> = vec![("k.toml".into(), MANIFEST.into())];
        match self {
            Pathological::DeepGenerics => deep_generics(size, &mut files),
            Pathological::HugeMatch => huge_match(size, &mut files),
            Pathological::ImportChain => import_chain(size, &mut files),
            Pathological::WideDi => wide_di(size, &mut files),
        }
        files
    }
}

/// Appends the files of [`Pathological::DeepGenerics`] with nesting
/// `depth`.
fn deep_generics(depth: usize, files: &mut Vec<File>) {
    files.push((
        "src/deep/Wrap.k".into(),
        "/// One level of wrapping.
public struct Wrap<T> {
    /// The wrapped value.
    let value: T

    /// Returns the wrapped value with `transform` applied.
    func map<U>(transform: (T) -> U) -> Wrap<U> {
        return Wrap(value: transform(value))
    }
}
"
        .into(),
    ));
    let mut ty = String::from("Int");
    let mut value = String::from("1");
    for _ in 0..depth {
        ty = format!("Wrap<{ty}>");
        value = format!("Wrap(value: {value})");
    }
    let mut t = String::new();
    for f in 0..40 {
        let _ = writeln!(t, "/// Builds a value nested {depth} deep.");
        let _ = writeln!(t, "func build{f}() -> {ty} {{");
        let _ = writeln!(t, "    let annotated: {ty} = {value}");
        let _ = writeln!(t, "    let inferred = {value}");
        let _ = writeln!(t, "    let mapped = inferred.map {{ inner => inner }}");
        let _ = writeln!(t, "    let unwrapped = mapped.value");
        let _ = writeln!(t, "    return annotated\n}}\n");
    }
    files.push(("src/deep/Builders.k".into(), t));
}

/// Appends the files of [`Pathological::HugeMatch`] with `cases` cases.
fn huge_match(cases: usize, files: &mut Vec<File>) {
    let mut t = String::from("/// An enum with very many cases.\npublic enum Huge {\n");
    for c in 0..cases {
        if c % 3 == 0 {
            let _ = writeln!(t, "    /// Case {c}.\n    case c{c}(value: Int)");
        } else {
            let _ = writeln!(t, "    /// Case {c}.\n    case c{c}");
        }
    }
    t.push_str(
        "\n    /// The number of each case.\n    var number: Int {\n        return match self {\n",
    );
    for c in 0..cases {
        if c % 3 == 0 {
            let _ = writeln!(t, "            case .c{c}(let value) {{ value + {c} }}");
        } else {
            let _ = writeln!(t, "            case .c{c} {{ {c} }}");
        }
    }
    t.push_str("        }\n    }\n\n    /// Whether the case is even.\n    func isEven() -> Bool {\n        return match self {\n");
    for c in 0..cases {
        let _ = writeln!(t, "            case .c{c} {{ {} }}", c % 2 == 0);
    }
    t.push_str("        }\n    }\n}\n");
    files.push(("src/huge/Huge.k".into(), t));
    let mut t = String::new();
    for f in 0..8 {
        let _ = writeln!(t, "/// Classifies a value, {f}.");
        let _ = writeln!(
            t,
            "func classify{f}(huge: Huge) -> String {{\n    return match huge {{"
        );
        for c in 0..cases {
            if c % 3 == 0 {
                let _ = writeln!(
                    t,
                    "        case .c{c}(let value) where value > {f} {{ \"big {c}\" }}"
                );
                let _ = writeln!(t, "        case .c{c} {{ \"small {c}\" }}");
            } else if c % 5 == 0 {
                let _ = writeln!(t, "        case .c{c} {{ \"case {c}\" }}");
            }
        }
        if cases >= 2 {
            t.push_str("        default { \"other\" }\n");
        }
        t.push_str("    }\n}\n\n");
    }
    files.push(("src/huge/Classify.k".into(), t));
}

/// Appends the files of [`Pathological::ImportChain`] with `length`
/// modules.
fn import_chain(length: usize, files: &mut Vec<File>) {
    for i in 0..length {
        let m = format!("chain{i:04}");
        let mut link = String::new();
        let mut service = String::new();
        if i > 0 {
            let p = format!("chain{:04}", i - 1);
            let _ = writeln!(link, "import {p}.Link{}\n", i - 1);
            let _ = writeln!(service, "import {p}.Step{}\n", i - 1);
        }
        let _ = writeln!(link, "/// Link {i} of the chain.\npublic struct Link{i} {{");
        let _ = writeln!(link, "    /// This link's weight.\n    let weight: Int");
        if i > 0 {
            let _ = writeln!(
                link,
                "\n    /// The previous link.\n    let previous: Link{}",
                i - 1
            );
            let _ = writeln!(
                link,
                "\n    /// The weight of the chain up to here.\n    func total() -> Int {{\n        return weight + previous.weight\n    }}"
            );
        } else {
            let _ = writeln!(
                link,
                "\n    /// The weight of the chain up to here.\n    func total() -> Int {{\n        return weight\n    }}"
            );
        }
        link.push_str("}\n");
        let _ = writeln!(
            service,
            "/// Step {i} of the service chain.\npublic service Step{i} {{"
        );
        if i > 0 {
            let _ = writeln!(service, "    @Inject let previous: Step{}\n", i - 1);
        }
        let _ = writeln!(
            service,
            "    /// This step's depth.\n    func depth() -> Int {{"
        );
        if i > 0 {
            let _ = writeln!(service, "        return previous.depth() + 1");
        } else {
            let _ = writeln!(service, "        return 0");
        }
        service.push_str("    }\n}\n");
        files.push((format!("src/{m}/Link{i}.k"), link));
        files.push((format!("src/{m}/Step{i}.k"), service));
    }
}

/// Appends the files of [`Pathological::WideDi`] with `width` branches.
fn wide_di(width: usize, files: &mut Vec<File>) {
    files.push((
        "src/wide/Leaf.k".into(),
        "/// The shared dependency of every branch.
public service Leaf {
    /// A constant.
    func value() -> Int {
        return 1
    }
}
"
        .into(),
    ));
    let mut root = String::from("/// Injects every branch.\npublic service Root {\n");
    for b in 0..width {
        files.push((
            format!("src/wide/Branch{b}.k"),
            format!(
                "/// Branch {b}.
public service Branch{b} {{
    @Inject let leaf: Leaf

    /// The branch's value.
    func value() -> Int {{
        return leaf.value() + {b}
    }}
}}
"
            ),
        ));
        let _ = writeln!(root, "    @Inject let branch{b}: Branch{b}");
    }
    root.push_str(
        "\n    /// The sum over every branch.\n    func total() -> Int {\n        var sum = 0\n",
    );
    for b in 0..width {
        let _ = writeln!(root, "        sum = sum + branch{b}.value()");
    }
    root.push_str("        return sum\n    }\n}\n");
    files.push(("src/wide/Root.k".into(), root));
    files.push((
        "src/wide/Overview.k".into(),
        "/// Shows the root's total.
public page Overview {
    @Inject let root: Root

    view {
        paragraph { \"Total: {root.total()}\" }
    }
}
"
        .into(),
    ));
}

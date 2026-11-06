//! Deterministic generators of K source text for benchmarks and budget checks.
//!
//! This module is shared, through `#[path]` includes, by
//! `benches/syntax.rs` (criterion throughput benchmarks) and by the tests
//! `tests/workload.rs` (the generators produce what they promise) and
//! `tests/perf_budget.rs` (the budgets of `docs/compiler/architecture.md`,
//! "Performance budgets"). Keeping one generator guarantees that the numbers
//! recorded in `docs/compiler/performance.md` and the numbers the budget
//! check enforces are measured on the same text.
//!
//! # Families of inputs
//!
//! * [`realistic`]: valid K of a requested number of lines that mixes every
//!   kind of declaration (components with views and styles, pages, layouts,
//!   services, protocols, entities, repositories, structs, enums, classes,
//!   interfaces, extensions, themes, apps, tests and top-level functions)
//!   with varied statements, expressions, literals, strings and comments.
//!   It parses without a single diagnostic; `tests/workload.rs` checks that
//!   for many seeds.
//! * [`typical_file`]: one realistic file of about 300 lines, the unit of the
//!   "a typical 300-line file in ≤ 0.5 ms" budget.
//! * [`Pathological`]: inputs that stress one dimension each (nesting depth,
//!   line length, string interpolation, error density, comments, non-ASCII
//!   text, long operator and member chains). Each is generated at a
//!   requested size so the budget check can verify that time grows linearly
//!   with it.
//!
//! # Determinism
//!
//! Every generator is a pure function of its arguments. Randomness comes
//! from [`Rng`], a SplitMix64 generator seeded by the caller, so the same
//! seed yields byte-identical text on every platform and every run; there
//! is no dependency on hash-map ordering, time or the environment.

#![allow(dead_code)]

use std::fmt::Write as _;

/// The seed used by the benchmarks and the budget check.
///
/// Changing it changes the measured text, so baselines recorded in
/// `docs/compiler/performance.md` are only comparable for the same seed.
pub const SEED: u64 = 0x4b5f_5359_4e54_4158; // "K_SYNTAX"

/// A small, fast, deterministic pseudo-random number generator (SplitMix64).
///
/// It is not cryptographically secure and is not meant to be: it only has to
/// be reproducible and well distributed enough to vary the generated text.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator whose sequence is fully determined by `seed`.
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// Returns the next 64 pseudo-random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..n`.
    ///
    /// # Panics
    ///
    /// Panics if `n` is zero (a bug in the caller, never input-dependent).
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "Rng::below(0)");
        (self.next_u64() % n as u64) as usize
    }

    /// Returns a value in `low..=high`.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    /// Returns `true` with a probability of `percent` / 100.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }

    /// Returns one element of `items`, chosen uniformly.
    pub fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

const NOUNS: &[&str] = &[
    "Cart",
    "Product",
    "Order",
    "Invoice",
    "Customer",
    "Review",
    "Shipment",
    "Coupon",
    "Wishlist",
    "Payment",
    "Account",
    "Session",
    "Report",
    "Ticket",
    "Message",
    "Catalog",
    "Profile",
    "Search",
    "Checkout",
    "Inventory",
];

const FIELDS: &[&str] = &[
    "title",
    "price",
    "quantity",
    "status",
    "createdAt",
    "owner",
    "total",
    "rating",
    "isActive",
    "count",
    "label",
    "summary",
    "items",
    "name",
    "email",
    "discount",
    "stock",
    "tags",
];

const LOCALS: &[&str] = &[
    "item", "value", "result", "entry", "product", "order", "user", "index", "total", "line",
    "draft", "next", "current", "response",
];

const TYPES: &[&str] = &[
    "Int",
    "String",
    "Bool",
    "Money",
    "Date",
    "Duration",
    "Color",
    "Float",
    "List<String>",
    "List<Int>",
    "Map<String, Int>",
    "Int?",
    "String?",
    "Set<ProductId>",
    "Result<Order, OrderError>",
    "(Int, String)",
    "(Product) -> Bool",
];

const ELEMENTS: &[&str] = &[
    "section",
    "box",
    "row",
    "column",
    "paragraph",
    "heading(level: 2)",
    "heading(level: 3)",
    "list",
    "card",
    "stack(gap: .md)",
    "label(for: .email)",
];

const CLASSES: &[&str] = &[
    "summary", "muted", "small", "expanded", "card", "header", "footer", "active", "primary",
    "danger", "grid", "title", "badge", "toolbar",
];

const PROPERTIES: &[&str] = &[
    "padding",
    "margin",
    "color",
    "backgroundColor",
    "fontSize",
    "borderRadius",
    "gap",
    "maxWidth",
    "display",
    "border",
    "transition",
    "fontFamily",
    "gridTemplateColumns",
    "opacity",
];

const CSS_VALUES: &[&str] = &[
    ".md",
    ".sm",
    ".lg",
    ".primary",
    ".surface",
    ".primary.opacity(0.8)",
    "1px solid .border",
    "4px solid {accent}",
    "flex",
    "none",
    "100%",
    "#3B82F6",
    "#fff",
    "calc(100% - 2px)",
    "\"Inter\", sans-serif",
    "200ms ease-in-out",
    "repeat(3, 1fr)",
    "-4px",
    "0.5",
    "{opacity}",
];

const WORDS: &[&str] = &[
    "Your", "cart", "is", "empty", "Pay", "now", "Nothing", "here", "yet", "Loading", "Saved",
    "items", "total", "Welcome", "back", "Order", "shipped", "on",
];

// ---------------------------------------------------------------------------
// Realistic input
// ---------------------------------------------------------------------------

/// Generates valid K source of at least `lines` lines (and at most one
/// declaration more), deterministically from `seed`.
///
/// The text starts with a block of imports and then cycles through every
/// kind of declaration in a seeded order, so any window of a few hundred
/// lines contains components, views, styles, services, protocols and plain
/// code. The average line is about 30 bytes, close to the 35 bytes per line
/// assumed by the project-size budgets.
///
/// The output parses without diagnostics (checked by `tests/workload.rs`).
pub fn realistic(lines: usize, seed: u64) -> String {
    let mut g = Gen::new(seed, lines * 32);
    g.imports();
    while g.lines < lines {
        g.item();
    }
    g.out
}

/// Generates one realistic file of about 300 lines.
///
/// This is the unit of the "a typical 300-line file in ≤ 0.5 ms" budget.
pub fn typical_file(seed: u64) -> String {
    realistic(300, seed)
}

/// Counts the lines of `text` the way an editor does: the number of line
/// terminators, plus one if the last line is not terminated.
pub fn line_count(text: &str) -> usize {
    let newlines = text.bytes().filter(|&b| b == b'\n').count();
    newlines + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

/// The state of the realistic generator.
struct Gen {
    out: String,
    rng: Rng,
    indent: usize,
    lines: usize,
    serial: u32,
}

impl Gen {
    fn new(seed: u64, capacity: usize) -> Gen {
        Gen {
            out: String::with_capacity(capacity),
            rng: Rng::new(seed),
            indent: 0,
            lines: 0,
            serial: 0,
        }
    }

    /// Writes one line at the current indentation.
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
        self.lines += 1;
    }

    /// Writes an empty line.
    fn blank(&mut self) {
        self.out.push('\n');
        self.lines += 1;
    }

    /// Writes `header {` and indents.
    fn open(&mut self, header: &str) {
        let line = format!("{header} {{");
        self.line(&line);
        self.indent += 1;
    }

    /// Dedents and writes `}`.
    fn close(&mut self) {
        self.indent -= 1;
        self.line("}");
    }

    /// Dedents and writes `}` followed by `suffix` (for `} else {`).
    fn close_with(&mut self, suffix: &str) {
        self.indent -= 1;
        let line = format!("}} {suffix} {{");
        self.line(&line);
        self.indent += 1;
    }

    /// A fresh number, so declarations get distinct names.
    fn serial(&mut self) -> u32 {
        self.serial += 1;
        self.serial
    }

    fn noun(&mut self) -> &'static str {
        self.rng.pick(NOUNS)
    }

    fn field(&mut self) -> &'static str {
        self.rng.pick(FIELDS)
    }

    fn local(&mut self) -> &'static str {
        self.rng.pick(LOCALS)
    }

    fn ty(&mut self) -> &'static str {
        self.rng.pick(TYPES)
    }

    // -- files and items ----------------------------------------------------

    fn imports(&mut self) {
        let count = self.rng.range(4, 12);
        for _ in 0..count {
            let noun = self.noun();
            let line = match self.rng.below(5) {
                0 => format!("import shop.{}.{noun}", noun.to_ascii_lowercase()),
                1 => format!("import shop.{}.{noun}Service", noun.to_ascii_lowercase()),
                2 => format!("import github.com/acme/ui-kit.{noun}Card"),
                3 => format!(
                    "import proto \"billing/v1/{}.proto\"",
                    noun.to_ascii_lowercase()
                ),
                _ => format!("import std.time.Duration{}", self.rng.below(9)),
            };
            self.line(&line);
        }
        self.blank();
    }

    fn item(&mut self) {
        if self.rng.chance(60) {
            let doc = format!(
                "/// {} {} for the {} screen.",
                self.rng.pick(WORDS),
                self.field(),
                self.noun()
            );
            self.line(&doc);
        } else if self.rng.chance(15) {
            self.line("/* Generated for benchmarks. /* Nested comments are allowed. */ */");
        }
        match self.rng.below(17) {
            0..=2 => self.component(),
            3 => self.page(),
            4 => self.layout(),
            5 | 6 => self.service(),
            7 => self.protocol(),
            8 => self.entity(),
            9 => self.repository(),
            10 => self.structure(),
            11 => self.enumeration(),
            12 => self.class_or_interface(),
            13 => self.theme_or_app(),
            14 => self.test_suite(),
            15 => self.extension(),
            _ => self.top_level(),
        }
        self.blank();
    }

    fn visibility(&mut self) -> &'static str {
        match self.rng.below(6) {
            0 => "public ",
            1 => "private ",
            _ => "",
        }
    }

    fn component(&mut self) {
        let name = format!(
            "{}{}{}",
            self.noun(),
            self.rng.pick(&["Card", "Row", "List", "Panel", "Summary"]),
            self.serial()
        );
        let header = format!("{}component {name}", self.visibility());
        self.open(&header);
        self.line("@Prop let accent: Color = .primary");
        for _ in 0..self.rng.range(1, 3) {
            let field = self.field();
            let ty = self.ty();
            let line = format!("@Prop let {field}{}: {ty}", self.serial());
            self.line(&line);
        }
        let service = format!("@Inject let store: {}Service", self.noun());
        self.line(&service);
        self.line("@State var isExpanded: Bool = false");
        let state = format!("@State var count: Int = {}", self.rng.below(10));
        self.line(&state);
        self.line(
            "@Derived let total: Money = store.items.sum { item => item.price * item.quantity }",
        );
        self.blank();
        self.func(false);
        self.blank();
        self.view();
        self.blank();
        self.style();
        self.close();
    }

    fn page(&mut self) {
        let noun = self.noun();
        let name = format!("{noun}Page{}", self.serial());
        let header = match self.rng.below(3) {
            0 => format!("page {name} at \"/{}/{{id}}\"", noun.to_ascii_lowercase()),
            1 => format!(
                "page {name} requires Authenticated, Role(.admin) at \"/admin/{}/{{id}}/{{tab}}\"",
                noun.to_ascii_lowercase()
            ),
            _ => format!(
                "public page {name} requires Authenticated at \"/{}\"",
                noun.to_ascii_lowercase()
            ),
        };
        self.open(&header);
        self.line("@Param let id: ProductId");
        let inject = format!("@Inject let api: {noun}Protocol");
        self.line(&inject);
        self.line("@State var tab: Tab = .overview");
        self.blank();
        self.open("loading");
        self.line("spinner(size: .lg)");
        self.close();
        self.open("failed(let error)");
        self.line("paragraph { \"Could not load: {error.message}\" }");
        self.line("    .class(.danger)");
        self.close();
        self.open("metadata");
        let title = format!("title(\"{noun} {{id}}\")");
        self.line(&title);
        self.line("canonical(.product(id: id).tab(.reviews))");
        self.close();
        self.view();
        self.close();
    }

    fn layout(&mut self) {
        let name = format!("{}Layout{}", self.noun(), self.serial());
        let header = if self.rng.chance(50) {
            format!("layout {name} requires Authenticated")
        } else {
            format!("layout {name}")
        };
        self.open(&header);
        self.open("view");
        self.open("column");
        self.line("Header(title: \"Shop\", user: session.user)");
        self.line("content");
        self.line("Footer()");
        self.close();
        self.line(".class(.grid)");
        self.close();
        self.close();
    }

    fn service(&mut self) {
        let noun = self.noun();
        let name = format!("{noun}Service{}", self.serial());
        let header = if self.rng.chance(50) {
            format!("service {name} implements {noun}Protocol")
        } else {
            format!("service {name}")
        };
        self.open(&header);
        let api = format!("@Inject let api: {noun}Protocol");
        self.line(&api);
        let items = format!("@State var items: List<{noun}Item> = []");
        self.line(&items);
        self.line("@State var lastError: String? = nil");
        self.blank();
        for _ in 0..self.rng.range(1, 3) {
            self.func(true);
            self.blank();
        }
        if self.rng.chance(50) {
            let rpc = format!("rpc fetch(id: {noun}Id) async throws {noun}Error -> {noun}");
            self.open(&rpc);
            self.line("let row = try await db.find(id: id)");
            self.line("return row ?? .placeholder");
            self.close();
        }
        self.close();
    }

    fn protocol(&mut self) {
        let noun = self.noun();
        let name = format!("{noun}Protocol{}", self.serial());
        let header = if self.rng.chance(40) {
            format!("protocol {name} requires Authenticated")
        } else {
            format!("protocol {name}")
        };
        self.open(&header);
        for _ in 0..self.rng.range(2, 5) {
            let field = self.field();
            let line = match self.rng.below(4) {
                0 => format!("rpc {field}(id: {noun}Id) throws {noun}Error -> {noun}"),
                1 => format!("rpc list{noun}(page: Int = 1, size: Int = 20) -> List<{noun}>"),
                2 => format!(
                    "rpc update{noun}(request: Update{noun}Request) async throws {noun}Error -> {noun} requires Role(.editor)"
                ),
                _ => format!("rpc delete{noun}(id: {noun}Id) throws {noun}Error"),
            };
            self.line(&line);
        }
        self.close();
    }

    fn entity(&mut self) {
        let noun = self.noun();
        let header = format!("entity {noun}Record{} implements Auditable", self.serial());
        self.open(&header);
        self.line("@Id let id: Uuid");
        for _ in 0..self.rng.range(2, 5) {
            let field = self.field();
            let ty = self.ty();
            let line = if self.rng.chance(30) {
                format!("@Index var {field}: {ty}")
            } else {
                format!("var {field}: {ty}")
            };
            self.line(&line);
        }
        self.line("@Default(.now) let createdAt: Date");
        self.close();
    }

    fn repository(&mut self) {
        let noun = self.noun();
        let header = format!(
            "repository {noun}Repository{} for {noun}Record",
            self.serial()
        );
        self.open(&header);
        let find = format!(
            "func byOwner(owner: UserId, limit: Int = 50) async throws DbError -> List<{noun}Record>"
        );
        self.open(&find);
        self.line("return try await query.where { row => row.owner == owner }.limit(limit).all()");
        self.close();
        self.close();
    }

    fn structure(&mut self) {
        let name = format!("{}Item{}", self.noun(), self.serial());
        let header = match self.rng.below(3) {
            0 => format!("struct {name}<T> implements Equatable where T implements Priced"),
            1 => format!("struct {name} implements Codable, Hashable"),
            _ => format!("{}struct {name}", self.visibility()),
        };
        self.open(&header);
        for _ in 0..self.rng.range(2, 6) {
            let field = self.field();
            let ty = self.ty();
            let line = format!("let {field}{}: {ty}", self.serial());
            self.line(&line);
        }
        self.blank();
        self.open("init(from other: Self) throws DecodeError");
        self.line("self.id = other.id");
        self.close();
        self.open("var isEmpty: Bool");
        self.line("count == 0");
        self.close();
        self.func(false);
        self.close();
    }

    fn enumeration(&mut self) {
        let noun = self.noun();
        let header = format!("enum {noun}Error{} implements Error", self.serial());
        self.open(&header);
        self.line("case notFound");
        self.line("case outOfStock(available: Int)");
        self.line("case invalid(field: String, reason: String)");
        self.line("case network(Int, String?)");
        self.blank();
        self.open("func describe() -> String");
        self.open("match self");
        self.line("case .notFound { \"Not found\" }");
        self.line(
            "case .outOfStock(let available) where available > 0 { \"Only {available} left\" }",
        );
        self.line("case .outOfStock(_) { \"Sold out\" }");
        self.line("case .invalid(field: let field, reason: _) { \"Invalid {field}\" }");
        self.line("default { \"Something went wrong\" }");
        self.close();
        self.close();
        self.close();
    }

    fn class_or_interface(&mut self) {
        let noun = self.noun();
        if self.rng.chance(50) {
            let header = format!(
                "interface {noun}Source{}<T> implements Sendable",
                self.serial()
            );
            self.open(&header);
            self.line("func load(id: T) async throws LoadError -> T?");
            self.line("func save(_ value: T) async throws SaveError");
            self.line("func observe(handler: (T) -> Void) -> Subscription");
            self.close();
        } else {
            let header = format!(
                "class {noun}Cache{}<K, V> implements Cache where K implements Hashable",
                self.serial()
            );
            self.open(&header);
            self.line("private var entries: Map<K, V> = [:]");
            self.line("static let shared: Self = Self()");
            self.blank();
            self.open("func get(key: K) -> V?");
            self.line("entries[key]");
            self.close();
            self.open("func put(key: K, value: V)");
            self.line("entries[key] = value");
            self.close();
            self.close();
        }
    }

    fn theme_or_app(&mut self) {
        if self.rng.chance(50) {
            let header = format!("theme Brand{}", self.serial());
            self.open(&header);
            self.line("color primary: Color = #3B82F6");
            self.line("color surface: Color = #ffffff");
            self.line("color muted: Color = #6B7280");
            self.line("space md: Length = 16px");
            self.line("space sm: Length = 0.5rem");
            self.line("radius card: Length = 12px");
            self.line("motion fast: Duration = 150ms");
            self.open("variant dark");
            self.line("primary = #60A5FA");
            self.line("surface = #111827");
            self.close();
            self.close();
        } else {
            let header = format!("app Shop{}", self.serial());
            self.open(&header);
            self.line("use StripePayments for PaymentProvider in .production");
            self.line("use FakePayments for PaymentProvider in .development, .test");
            self.line("use PostgresDatabase for Database");
            self.line("let theme = Brand");
            self.close();
        }
    }

    fn test_suite(&mut self) {
        let noun = self.noun();
        let header = format!("test {noun}Service{}", self.serial());
        self.open(&header);
        let fake = format!("use Fake{noun}Api for {noun}Protocol");
        self.line(&fake);
        let inject = format!("@Inject let service: {noun}Service");
        self.line(&inject);
        self.blank();
        let case = format!(
            "test \"adding a {} updates the total\"",
            noun.to_ascii_lowercase()
        );
        self.open(&case);
        self.line("try await service.add(product: .sample, quantity: 2)");
        self.line("expect service.items.count == 1");
        self.line("expect service.total == 19.98.usd");
        self.close();
        self.open("test \"out of stock surfaces a typed error\"");
        let throws = format!("expect throws {noun}Error.outOfStock(available: 0)");
        self.open(&throws);
        self.line("try await service.add(product: .soldOut, quantity: 1)");
        self.close();
        self.close();
        self.close();
    }

    fn extension(&mut self) {
        if self.rng.chance(50) {
            self.open("extend<T> List<T> where T implements Priced");
            self.open("func cheapest() -> T?");
            self.line("self.min { a, b => a.price < b.price }");
            self.close();
            self.close();
        } else {
            let header = format!("extend {}", self.noun());
            self.open(&header);
            self.open("var displayName: String");
            self.line("\"{name} ({count})\"");
            self.close();
            self.close();
        }
    }

    fn top_level(&mut self) {
        if self.rng.chance(30) {
            let line = format!(
                "let defaultTimeout{} = {}",
                self.serial(),
                self.rng.pick(&[
                    "30.seconds",
                    "250ms",
                    "2.5e3",
                    "0x1F_FF",
                    "0b1010",
                    "[:]",
                    "[1, 2, 3]"
                ])
            );
            self.line(&line);
        } else {
            let may_throw = self.rng.chance(50);
            self.func(may_throw);
        }
    }

    // -- functions and statements --------------------------------------------

    fn func(&mut self, may_throw: bool) {
        let name = format!(
            "{}{}",
            self.rng.pick(&[
                "load", "save", "update", "compute", "handle", "format", "apply"
            ]),
            self.serial()
        );
        let params = match self.rng.below(5) {
            0 => String::new(),
            1 => format!("id: {}Id", self.noun()),
            2 => "for product: ProductId, quantity: Int = 1".to_owned(),
            3 => format!(
                "_ items: List<{}>, where predicate: (Item) -> Bool",
                self.noun()
            ),
            _ => "@Validate(.positive) amount: Money, note: String? = nil".to_owned(),
        };
        let effects = if may_throw {
            format!(" async throws {}Error", self.noun())
        } else {
            String::new()
        };
        let ret = if self.rng.chance(50) {
            format!(" -> {}", self.ty())
        } else {
            String::new()
        };
        let header = format!("{}func {name}({params}){effects}{ret}", self.visibility());
        self.open(&header);
        let count = self.rng.range(3, 7);
        for _ in 0..count {
            self.statement(2, may_throw);
        }
        self.close();
    }

    fn statement(&mut self, depth: usize, may_throw: bool) {
        let choice = if depth == 0 {
            self.rng.below(5)
        } else {
            self.rng.below(14)
        };
        match choice {
            0 => {
                let local = self.local();
                let value = self.value_expr(2);
                let line = format!("let {local} = {value}");
                self.line(&line);
            }
            1 => {
                let local = self.local();
                let ty = self.ty();
                let value = self.expr(2);
                let line = format!("var {local}{}: {ty} = {value}", self.serial());
                self.line(&line);
            }
            2 => {
                let target = format!("{}.{}", self.local(), self.field());
                let op = self.rng.pick(&["=", "+=", "-=", "*=", "/=", "%="]);
                let value = self.expr(2);
                let line = format!("{target} {op} {value}");
                self.line(&line);
            }
            3 => {
                let call = self.call_expr(2);
                self.line(&call);
            }
            4 => {
                if self.rng.chance(20) {
                    self.line("// Keep the cache warm for the next request.");
                } else {
                    let value = self.expr(1);
                    let line = format!("return {value}");
                    self.line(&line);
                }
            }
            5 | 6 => {
                let cond = self.condition();
                let header = format!("if {cond}");
                self.open(&header);
                self.statement(depth - 1, may_throw);
                if self.rng.chance(50) {
                    let cond = self.condition();
                    self.close_with(&format!("else if {cond}"));
                    self.statement(depth - 1, may_throw);
                }
                if self.rng.chance(60) {
                    self.close_with("else");
                    self.statement(depth - 1, may_throw);
                }
                self.close();
            }
            7 => {
                let local = self.local();
                let iterable = self.nt_expr(1);
                let header = if self.rng.chance(30) {
                    format!("for ({local}, offset) in {iterable}.enumerated()")
                } else {
                    format!("for {local} in {iterable}")
                };
                self.open(&header);
                self.statement(depth - 1, may_throw);
                if self.rng.chance(30) {
                    let jump = self.rng.pick(&["break", "continue"]);
                    self.line(jump);
                }
                self.close();
            }
            8 => {
                let cond = self.condition();
                let header = format!("while {cond}");
                self.open(&header);
                self.statement(depth - 1, may_throw);
                self.close();
            }
            9 => {
                let local = self.local();
                let source = format!("{}.{}", self.local(), self.field());
                let header = format!("guard let {local} = {source}, {local}.isValid else");
                self.open(&header);
                self.line("return");
                self.close();
            }
            10 => {
                self.open("do");
                let call = self.call_expr(1);
                let line = format!("try await {call}");
                self.line(&line);
                self.close_with("catch .notFound, .outOfStock(_)");
                self.line("lastError = \"Unavailable\"");
                self.close_with("catch let error where error.isRetryable");
                self.line("retry(after: 2.seconds)");
                self.close_with("catch _");
                self.line("throw .unknown");
                self.close();
            }
            11 => {
                let subject = self.nt_expr(1);
                let header = format!("match {subject}");
                self.open(&header);
                self.line("case .loaded(let value), .cached(let value) { show(value) }");
                self.line("case 0 ..< 10 { small() }");
                self.line("case \"a\", \"b\" { letter() }");
                self.open("default");
                self.statement(depth - 1, may_throw);
                self.close();
                self.close();
            }
            12 => {
                let local = self.local();
                let call = self.call_expr(1);
                let line = if may_throw {
                    format!("let {local} = try await {call}")
                } else {
                    format!("let {local} = try {call} else {{ fallback }}")
                };
                self.line(&line);
            }
            _ => {
                let local = self.local();
                let line = format!(
                    "let {local} = items.filter {{ item => item.stock > 0 and not item.hidden }}\
                     .map {{ (item: Item) -> Int => item.price * 2 }}"
                );
                self.line(&line);
            }
        }
    }

    // -- expressions -------------------------------------------------------

    /// An expression that may be an `if` or `match` expression (only in
    /// positions that end the statement, such as a `let` initializer).
    fn value_expr(&mut self, depth: usize) -> String {
        match self.rng.below(10) {
            0 => {
                let cond = self.condition();
                let a = self.expr(1);
                let b = self.expr(1);
                format!("if {cond} {{ {a} }} else {{ {b} }}")
            }
            _ => self.expr(depth),
        }
    }

    /// A general expression: arithmetic, comparisons, logic, `??`, ranges.
    fn expr(&mut self, depth: usize) -> String {
        self.expr_in(depth, false)
    }

    /// An expression without a trailing lambda at its top level (`expr_nt`).
    fn nt_expr(&mut self, depth: usize) -> String {
        self.atom_in(depth, true)
    }

    fn expr_in(&mut self, depth: usize, nt: bool) -> String {
        match self.rng.below(8) {
            0..=2 => self.atom_in(depth, nt),
            3 | 4 => {
                // Arithmetic chain; `-` never starts it, `%` is spaced.
                let mut text = self.atom_in(depth, nt);
                for _ in 0..self.rng.range(1, 3) {
                    let op = self.rng.pick(&["+", "-", "*", "/", "%"]);
                    let rhs = self.atom_in(depth, nt);
                    let _ = write!(text, " {op} {rhs}");
                }
                text
            }
            5 => {
                let a = self.atom_in(depth, nt);
                let op = self.rng.pick(&["==", "!=", "<", "<=", ">", ">="]);
                let b = self.atom_in(depth, nt);
                format!("{a} {op} {b}")
            }
            6 => {
                let a = self.atom_in(depth, nt);
                let b = self.atom_in(depth, nt);
                format!("{a} ?? {b}")
            }
            _ => {
                let a = self.atom_in(depth, nt);
                let op = self.rng.pick(&["..<", "..."]);
                let b = self.atom_in(depth, nt);
                format!("{a} {op} {b}")
            }
        }
    }

    /// A condition for `if`, `while` and `guard`: no trailing lambdas.
    fn condition(&mut self) -> String {
        match self.rng.below(6) {
            0 => {
                let local = self.local();
                let source = format!("{}.{}", self.local(), self.field());
                format!("let {local} = {source}")
            }
            1 => {
                let a = self.nt_expr(1);
                let b = self.nt_expr(1);
                format!("not {a}.isEmpty and {b} > 0")
            }
            2 => {
                let a = self.nt_expr(1);
                format!("{a} == nil or isExpanded")
            }
            3 => "(items.contains { item => item.isNew })".to_owned(),
            _ => {
                let a = self.nt_expr(1);
                let op = self.rng.pick(&["<", ">=", "!="]);
                let b = self.nt_expr(1);
                format!("{a} {op} {b}")
            }
        }
    }

    /// A call, often with named arguments and a trailing lambda.
    fn call_expr(&mut self, depth: usize) -> String {
        self.call_expr_in(depth, false)
    }

    /// A call; with `nt`, never with a trailing lambda.
    fn call_expr_in(&mut self, depth: usize, nt: bool) -> String {
        let receiver = format!("{}.{}", self.local(), self.field());
        let method = self
            .rng
            .pick(&["update", "send", "append", "track", "load", "log"]);
        let mut args = String::new();
        for i in 0..self.rng.range(0, 3) {
            if i > 0 {
                args.push_str(", ");
            }
            if self.rng.chance(60) {
                let label = self.field();
                let _ = write!(args, "{label}: ");
            }
            let value = self.expr(depth.saturating_sub(1));
            args.push_str(&value);
        }
        if !nt && self.rng.chance(30) {
            let local = self.local();
            let body = self.expr(depth.saturating_sub(1));
            format!("{receiver}.{method}({args}) {{ {local} => {body} }}")
        } else {
            format!("{receiver}.{method}({args})")
        }
    }

    /// An operand: a literal, a name, a postfix chain or a bracketed form.
    fn atom_in(&mut self, depth: usize, nt: bool) -> String {
        let limit = if depth == 0 { 9 } else { 18 };
        match self.rng.below(limit) {
            0 => self.rng.below(10_000).to_string(),
            1 => self
                .rng
                .pick(&[
                    "3.14",
                    "0.25",
                    "1e9",
                    "2.5e-3",
                    "1_000_000",
                    "0xFF",
                    "0b1010",
                    "0o755",
                ])
                .to_owned(),
            2 => self
                .rng
                .pick(&[
                    "16px", "1.5rem", "200ms", "2s", "50%", "45deg", "1fr", "100vh",
                ])
                .to_owned(),
            3 => self
                .rng
                .pick(&[
                    "#fff",
                    "#3B82F6",
                    "#3B82F680",
                    "true",
                    "false",
                    "nil",
                    "self",
                ])
                .to_owned(),
            4 => self.string(depth),
            5 | 6 => {
                let local = self.local();
                let field = self.field();
                format!("{local}.{field}")
            }
            7 => {
                let a = self
                    .rng
                    .pick(&["primary", "md", "currency", "loaded", "none"]);
                format!(".{a}")
            }
            8 => {
                let local = self.local();
                let field = self.field();
                format!("{local}?.{field} ?? 0")
            }
            9 => {
                let inner = self.expr(depth - 1);
                format!("({inner})")
            }
            10 => {
                let a = self.expr(depth - 1);
                let b = self.expr(depth - 1);
                format!("[{a}, {b}]")
            }
            11 => {
                let a = self.expr(depth - 1);
                format!("[\"{}\": {a}, \"limit\": 20]", self.field())
            }
            12 => {
                let a = self.expr(depth - 1);
                let b = self.expr(depth - 1);
                format!("({a}, {b})")
            }
            13 => {
                let a = self.expr(depth - 1);
                format!("(not {a})")
            }
            14 => {
                let local = self.local();
                format!("-{local}.offset")
            }
            15 => {
                let local = self.local();
                let index = self.rng.below(8);
                format!("{local}.items[{index}].price")
            }
            16 => {
                if nt {
                    let local = self.local();
                    format!("{local}.values.count")
                } else {
                    let local = self.local();
                    let body = self.expr(depth - 1);
                    format!("{local}s.map {{ {local} => {body} }}")
                }
            }
            _ => self.call_expr_in(depth - 1, nt),
        }
    }

    /// A string literal with text, escapes and interpolations.
    fn string(&mut self, depth: usize) -> String {
        let mut text = String::from("\"");
        for i in 0..self.rng.range(1, 4) {
            if i > 0 {
                text.push(' ');
            }
            match self.rng.below(7) {
                0 if depth > 0 => {
                    let inner = self.atom_in(depth - 1, false);
                    let _ = write!(text, "{{{inner}}}");
                }
                1 => {
                    let local = self.local();
                    let _ = write!(text, "{{{local}.{}}}", self.field());
                }
                2 => text.push_str(self.rng.pick(&[
                    "\\n",
                    "\\t",
                    "\\\"",
                    "\\u{1F600}",
                    "{{literal}}",
                    "\\\\",
                ])),
                3 => text.push_str(self.rng.pick(&[
                    "caf\u{e9}",
                    "\u{4e2d}\u{6587}",
                    "\u{1F6D2} cart",
                    "na\u{ef}ve",
                ])),
                _ => {
                    let a = self.rng.pick(WORDS);
                    let b = self.rng.pick(WORDS);
                    let _ = write!(text, "{a} {b}");
                }
            }
        }
        text.push('"');
        text
    }

    // -- views and styles --------------------------------------------------

    fn view(&mut self) {
        self.open("view");
        let root = self.rng.pick(&["section", "column", "card"]);
        self.open(root);
        for _ in 0..self.rng.range(2, 4) {
            let depth = self.rng.range(1, 3);
            self.view_node(depth);
        }
        self.close();
        let class = self.rng.pick(CLASSES);
        let line = format!(".class([.{class}, .expanded when isExpanded])");
        self.line(&line);
        self.close();
    }

    fn view_node(&mut self, depth: usize) {
        let choice = if depth == 0 {
            self.rng.below(4)
        } else {
            self.rng.below(9)
        };
        match choice {
            0 => {
                let s = self.string(1);
                self.line(&s);
            }
            1 => {
                let noun = self.noun();
                let arg = self.expr(1);
                let line = format!("{noun}Row(item: {arg}, quantity: bind count)");
                self.line(&line);
            }
            2 => {
                let s = self.string(1);
                let line = format!("paragraph {{ {s} }}");
                self.line(&line);
                let class = self.rng.pick(CLASSES);
                let line = format!("    .class(.{class})");
                self.line(&line);
            }
            3 => {
                let s = self.string(1);
                let line = format!("button {{ {s} }}");
                self.line(&line);
                self.line("    .type(.submit)");
                self.line("    .on(click: toggle)");
                self.line("    .disabled(not isExpanded)");
            }
            4 | 5 => {
                let element = self.rng.pick(ELEMENTS);
                self.open(element);
                for _ in 0..self.rng.range(1, 3) {
                    self.view_node(depth - 1);
                }
                self.close();
                let class = self.rng.pick(CLASSES);
                let line = format!(".class(.{class})");
                self.line(&line);
            }
            6 => {
                let cond = self.condition();
                let header = format!("if {cond}");
                self.open(&header);
                self.view_node(depth - 1);
                self.close_with("else");
                self.view_node(depth - 1);
                self.close();
            }
            7 => {
                let header = format!("for item in store.{} key item.id", self.field());
                self.open(&header);
                self.view_node(depth - 1);
                self.close();
            }
            _ => {
                self.open("match state");
                self.line("case .loading { spinner() }");
                self.open("case .loaded(let data)");
                self.view_node(depth - 1);
                self.close();
                self.line("case .failed(let error) { ErrorBanner(error: error) }");
                self.close();
            }
        }
    }

    fn style(&mut self) {
        self.open("style");
        for _ in 0..self.rng.range(2, 4) {
            self.style_rule(2);
        }
        if self.rng.chance(50) {
            self.open("@media (maxWidth: 600px)");
            self.style_rule(0);
            self.close();
        }
        self.close();
    }

    fn style_rule(&mut self, depth: usize) {
        let a = self.rng.pick(CLASSES);
        let b = self.rng.pick(CLASSES);
        let selector = match self.rng.below(8) {
            0 => format!(".{a}"),
            1 => format!(".{a}:hover, .{a}:focus-visible"),
            2 => format!(".{a} > .{b}"),
            3 => format!(".{a} .{b}"),
            4 => format!(".{a}:nth-child(2n + 1)"),
            5 => format!(".{a}::before"),
            6 => format!(".{a}[disabled], .{a}:not(.{b})"),
            _ => format!(".{a} + .{b} ~ *"),
        };
        self.open(&selector);
        for _ in 0..self.rng.range(1, 4) {
            let property = self.rng.pick(PROPERTIES);
            let value = self.rng.pick(CSS_VALUES);
            let line = format!("{property}: {value};");
            self.line(&line);
        }
        if depth > 0 && self.rng.chance(50) {
            self.open("&:hover");
            self.line("backgroundColor: .surfaceHover;");
            self.close();
        }
        if depth > 1 && self.rng.chance(30) {
            self.open("@media (prefersReducedMotion: reduce)");
            self.line("transition: none;");
            self.close();
        }
        self.close();
    }
}

// ---------------------------------------------------------------------------
// Pathological input
// ---------------------------------------------------------------------------

/// Inputs that each stress one dimension of the lexer or the parser.
///
/// [`Pathological::generate`] produces an input of roughly `size` units (the
/// unit depends on the family and is documented on each variant). The budget
/// check generates every family at two sizes and verifies that the time
/// grows linearly; the benchmarks report their throughput.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pathological {
    /// Parenthesized expressions nested 300 levels deep (within the limit
    /// of 400), one per line; `size` is the number of lines. Valid.
    DeepParens,
    /// One expression nested `size` levels deep, far past the limit: the
    /// excess is reported once (K0134) and skipped iteratively.
    TooDeep,
    /// `if` blocks nested 150 levels deep inside a function, repeated;
    /// `size` is the number of functions. Valid.
    DeepBlocks,
    /// View elements nested 150 levels deep, repeated; `size` is the number
    /// of components. Valid.
    DeepView,
    /// Style rules nested 150 levels deep, repeated; `size` is the number of
    /// components. Valid.
    DeepStyle,
    /// One line holding a list literal with `size` elements. Valid.
    LongLine,
    /// One string literal of `size` bytes of text. Valid.
    LongString,
    /// One string with `size` interpolations. Valid.
    HugeInterpolation,
    /// Interpolations nested inside strings 100 levels deep, one per line;
    /// `size` is the number of lines. Valid.
    NestedInterpolation,
    /// `size` lines, each with one or more diagnostics of a different kind
    /// (lexer and parser errors, many with quick fixes).
    ManyDiagnostics,
    /// One line holding `size` repetitions of a broken fragment (`!`, `&&`,
    /// `;`, stray closers and operators): every error and quick fix on the
    /// same line, which once made the line-start lookups of the fix helpers
    /// quadratic.
    LongBrokenLine,
    /// `size` lines of comments: line, doc and nested block comments.
    CommentHeavy,
    /// `size` lines of strings full of non-ASCII text. Valid.
    UnicodeStrings,
    /// A member chain `x.m.m.m...` of `size` links. Valid.
    MemberChain,
    /// A sum `x + x + x...` of `size` terms. Valid.
    LongSum,
    /// `size` style-value interpolations nested in each other
    /// (`a: {style { a: {style { ...`), followed by `size` lines that each
    /// open an unterminated string. Every line break then ends a string
    /// above a deep stack of lexer frames, which once made the lexer and
    /// the parser search that stack once per line (quadratic).
    StringsInDeepStyles,
}

impl Pathological {
    /// Every family, in a stable order.
    pub const ALL: [Pathological; 16] = [
        Pathological::DeepParens,
        Pathological::TooDeep,
        Pathological::DeepBlocks,
        Pathological::DeepView,
        Pathological::DeepStyle,
        Pathological::LongLine,
        Pathological::LongString,
        Pathological::HugeInterpolation,
        Pathological::NestedInterpolation,
        Pathological::ManyDiagnostics,
        Pathological::LongBrokenLine,
        Pathological::CommentHeavy,
        Pathological::UnicodeStrings,
        Pathological::MemberChain,
        Pathological::LongSum,
        Pathological::StringsInDeepStyles,
    ];

    /// A short, stable, kebab-case name used as the benchmark id.
    pub fn name(self) -> &'static str {
        match self {
            Pathological::DeepParens => "deep-parens",
            Pathological::TooDeep => "too-deep",
            Pathological::DeepBlocks => "deep-blocks",
            Pathological::DeepView => "deep-view",
            Pathological::DeepStyle => "deep-style",
            Pathological::LongLine => "long-line",
            Pathological::LongString => "long-string",
            Pathological::HugeInterpolation => "huge-interpolation",
            Pathological::NestedInterpolation => "nested-interpolation",
            Pathological::ManyDiagnostics => "many-diagnostics",
            Pathological::LongBrokenLine => "long-broken-line",
            Pathological::CommentHeavy => "comment-heavy",
            Pathological::UnicodeStrings => "unicode-strings",
            Pathological::MemberChain => "member-chain",
            Pathological::LongSum => "long-sum",
            Pathological::StringsInDeepStyles => "strings-in-deep-styles",
        }
    }

    /// Whether the generated input is valid K (parses without diagnostics).
    pub fn is_valid(self) -> bool {
        !matches!(
            self,
            Pathological::TooDeep
                | Pathological::ManyDiagnostics
                | Pathological::LongBrokenLine
                | Pathological::StringsInDeepStyles
        )
    }

    /// A size that yields roughly 1 MB of text for this family.
    pub fn megabyte_size(self) -> usize {
        match self {
            Pathological::DeepParens => 1_700,
            Pathological::TooDeep => 500_000,
            Pathological::DeepBlocks => 25,
            Pathological::DeepView => 14,
            Pathological::DeepStyle => 13,
            Pathological::LongLine => 160_000,
            Pathological::LongString => 1_000_000,
            Pathological::HugeInterpolation => 120_000,
            Pathological::NestedInterpolation => 1_700,
            Pathological::ManyDiagnostics => 40_000,
            Pathological::LongBrokenLine => 32_000,
            Pathological::CommentHeavy => 25_000,
            Pathological::UnicodeStrings => 12_000,
            Pathological::MemberChain => 500_000,
            Pathological::LongSum => 250_000,
            Pathological::StringsInDeepStyles => 60_000,
        }
    }

    /// Generates the input at `size` (see the variant for the unit).
    pub fn generate(self, size: usize) -> String {
        match self {
            Pathological::DeepParens => {
                let depth = 300;
                let line = format!("let a = {}x{}\n", "(".repeat(depth), ")".repeat(depth));
                line.repeat(size)
            }
            Pathological::TooDeep => format!("let a = {}x{}\n", "(".repeat(size), ")".repeat(size)),
            Pathological::DeepBlocks => {
                let depth = 150;
                let mut out = String::new();
                for f in 0..size {
                    let _ = writeln!(out, "func f{f}() {{");
                    for d in 0..depth {
                        let _ = writeln!(out, "{:w$}if a{d} > {d} {{", "", w = (d + 1) * 2);
                    }
                    let _ = writeln!(out, "{:w$}work()", "", w = (depth + 1) * 2);
                    for d in (0..depth).rev() {
                        let _ = writeln!(out, "{:w$}}}", "", w = (d + 1) * 2);
                    }
                    out.push_str("}\n");
                }
                out
            }
            Pathological::DeepView => {
                let depth = 150;
                let mut out = String::new();
                for c in 0..size {
                    let _ = writeln!(out, "component Deep{c} {{\n  view {{");
                    for d in 0..depth {
                        let _ = writeln!(out, "{:w$}box {{", "", w = (d + 2) * 2);
                    }
                    let _ = writeln!(out, "{:w$}\"leaf {{count}}\"", "", w = (depth + 2) * 2);
                    for d in (0..depth).rev() {
                        let _ = writeln!(out, "{:w$}}}", "", w = (d + 2) * 2);
                        let _ = writeln!(out, "{:w$}.class(.level)", "", w = (d + 2) * 2);
                    }
                    out.push_str("  }\n}\n");
                }
                out
            }
            Pathological::DeepStyle => {
                let depth = 150;
                let mut out = String::new();
                for c in 0..size {
                    let _ = writeln!(out, "component Styled{c} {{\n  style {{");
                    for d in 0..depth {
                        let _ = writeln!(out, "{:w$}& .level{d}:hover {{", "", w = (d + 2) * 2);
                        let _ = writeln!(out, "{:w$}padding: .md;", "", w = (d + 3) * 2);
                    }
                    for d in (0..depth).rev() {
                        let _ = writeln!(out, "{:w$}}}", "", w = (d + 2) * 2);
                    }
                    out.push_str("  }\n}\n");
                }
                out
            }
            Pathological::LongLine => {
                let mut out = String::with_capacity(size * 7 + 16);
                out.push_str("let a = [");
                for i in 0..size {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(out, "{}", i % 9973);
                }
                out.push_str("]\n");
                out
            }
            Pathological::LongString => {
                let mut out = String::with_capacity(size + 16);
                out.push_str("let s = \"");
                let words = "lorem ipsum dolor sit amet ";
                while out.len() < size + 9 {
                    out.push_str(words);
                }
                out.push_str("\"\n");
                out
            }
            Pathological::HugeInterpolation => {
                let mut out = String::with_capacity(size * 9 + 16);
                out.push_str("let s = \"");
                for i in 0..size {
                    let _ = write!(out, "{{v{}}} ", i % 97);
                }
                out.push_str("\"\n");
                out
            }
            Pathological::NestedInterpolation => {
                let depth = 100;
                let line = format!("let s = {}x{}\n", "\"{".repeat(depth), "}\"".repeat(depth));
                line.repeat(size)
            }
            Pathological::ManyDiagnostics => {
                const LINES: &[&str] = &[
                    "let a = !b && c || d",
                    "let t = ready ? 1 : 2",
                    "let s = \"unterminated {x",
                    "let c = #ggg + 0xZZ + 1_",
                    "let x = 1; let y = 2;",
                    "$$$ ^^^ `` \\",
                    "func f( { ) ]",
                    "let e = \"bad \\q escape \\{ here\"",
                    "case a, b, c",
                    "let r = a < b < c",
                    "@ let q: Int = 1",
                    "public public let p = 1",
                ];
                let mut out = String::new();
                for i in 0..size {
                    out.push_str(LINES[i % LINES.len()]);
                    out.push('\n');
                }
                out
            }
            Pathological::LongBrokenLine => {
                let mut out = "let a = !b && c; x = 1; ) ] + ".repeat(size);
                out.push('\n');
                out
            }
            Pathological::CommentHeavy => {
                let mut out = String::new();
                for i in 0..size {
                    match i % 4 {
                        0 => {
                            out.push_str("// A line comment that explains the next declaration.\n")
                        }
                        1 => out.push_str("/// A doc comment with `code` and *emphasis* in it.\n"),
                        2 => out
                            .push_str("/* block /* nested /* deeper */ */ comment */ let x = 1\n"),
                        _ => out.push_str("    \t   // indented comment after whitespace\n"),
                    }
                }
                out
            }
            Pathological::UnicodeStrings => {
                let mut out = String::new();
                for i in 0..size {
                    let _ = writeln!(
                        out,
                        "let s{i} = \"\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} \u{1F6D2}\u{1F4E6} caf\u{e9} \u{0928}\u{092e}\u{0938}\u{094d}\u{0924}\u{0947} {{name}} \u{043f}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}\""
                    );
                }
                out
            }
            Pathological::MemberChain => format!("let a = x{}\n", ".m".repeat(size)),
            Pathological::LongSum => {
                let mut out = String::with_capacity(size * 4 + 16);
                out.push_str("let a = x");
                for _ in 1..size {
                    out.push_str(" + x");
                }
                out.push('\n');
                out
            }
            Pathological::StringsInDeepStyles => {
                let mut out = String::with_capacity(size * 15 + 16);
                out.push_str("let s = style { ");
                out.push_str(&"a: {style { ".repeat(size));
                out.push('\n');
                out.push_str(&"\"x\n".repeat(size));
                out
            }
        }
    }
}

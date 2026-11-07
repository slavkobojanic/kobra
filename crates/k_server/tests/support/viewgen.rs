//! **Generated views**, for the property "server HTML, parsed and
//! hydrated, equals the client render" (`docs/compiler/runtime.md` §6,
//! `docs/compiler/codegen.md` §11, `docs/compiler/testing.md`, "Stage 3
//! properties").
//!
//! [`generate`] writes one component with `String`, `Int`, `Bool` and
//! `List<String>` props and a view of nested elements (`box`, `section`,
//! `article`, `paragraph`, `text`, `emphasis`, `heading`, `list`/`item`,
//! `button`), static text with characters that need escaping, dynamic text
//! and interpolations of the props, `if`/`else` over a `Bool` prop or a
//! comparison, keyed `for` loops over the list prop, and dynamic `.label`
//! attributes. It also picks two sets of prop values with markup in the
//! strings (`<script>`, `-->`, `&amp;`, quotes, the empty string).
//!
//! [`check`] compiles the component and, in one isolate with the fake DOM:
//!
//! 1. renders it on the server with the first props (`Component$ssr`);
//! 2. parses that HTML into a container and serializes it back: it must be
//!    the same text (escaping is complete and the markup well-formed);
//! 3. renders it on the client into a second container: the same HTML;
//! 4. hydrates the first container: the same HTML, and the **same nodes**
//!    (hydration claims, it doesn't re-create; it may only add the empty
//!    text nodes an empty dynamic text has on the client but not in HTML);
//! 5. sets both instances' props to the second values: the hydrated and the
//!    client-rendered DOM stay equal, and equal a fresh server render of the
//!    second props.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::time::Duration;

use arbitrary::Unstructured;
use k_codegen::{FileKind, Options, Profile};
use k_server::{Engine, EngineOptions, JsModule};

use crate::coracle;
use crate::exprgen::string_literal;

/// Strings for props and static text: markup that must be escaped, the
/// empty string, and non-ASCII text.
pub const TEXTS: &[&str] = &[
    "plain",
    "",
    "<script>alert(1)</script>",
    "a & b",
    "&amp;",
    "\"quoted\" 'single'",
    "<!-- c -->",
    "-->",
    "</p>",
    "x < y > z",
    "\u{e9}t\u{e9} \u{1F642}",
    " ",
    "{braces}",
    "line\nbreak",
];

/// Findings this generator steers around, each pinned by a pending test;
/// when it is fixed, the test fails and the entry and the workaround go
/// together. None is open: `root-block-hydration`, `block-content-is-claimed`
/// and `template-text-nodes-merge` are fixed and guarded by
/// `fixed_findings_stay_fixed` in `crates/k_server/tests/differential.rs`,
/// so views now have block roots, bare texts and blocks at the top of
/// branches, empty static texts and adjacent texts.
pub const KNOWN_FINDINGS: &[(&str, &str)] = &[];

/// A text part inside an element.
#[derive(Clone, Debug)]
pub enum TextPart {
    /// A string literal.
    Static(String),
    /// The `String` prop.
    Prop,
    /// `"lit {n} lit {s} {b}"`.
    Interp(String),
    /// The loop variable of the enclosing `for`.
    Item,
}

/// A view node.
#[derive(Clone, Debug)]
pub enum Node {
    /// An element with optional dynamic `.label` and children.
    Element {
        /// The K element name (`box`, `heading(level: 2)`).
        name: String,
        /// `.label(title)` when it is a landmark.
        label: bool,
        /// The children.
        children: Vec<Node>,
    },
    /// A text part.
    Text(TextPart),
    /// `if cond { … } else { … }`.
    If {
        /// The condition, a K expression over the props.
        cond: String,
        /// The `then` children.
        then: Vec<Node>,
        /// The `else` children, if any.
        otherwise: Option<Vec<Node>>,
    },
    /// `list { for x in items key x { item { … } } }`.
    For(Vec<Node>),
}

/// One set of prop values.
#[derive(Clone, Debug, PartialEq)]
pub struct Props {
    /// `title`.
    pub title: String,
    /// `count`.
    pub count: i64,
    /// `isOn`.
    pub is_on: bool,
    /// `items` (unique).
    pub items: Vec<String>,
}

/// A generated case.
#[derive(Clone, Debug)]
pub struct Case {
    /// The view.
    pub view: Vec<Node>,
    /// The first props.
    pub first: Props,
    /// The props after the update.
    pub second: Props,
    /// The project.
    pub files: Vec<(String, String)>,
}

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    budget: u32,
}

const CONTAINERS: &[&str] = &["box", "section", "article", "box"];

impl Gen<'_, '_> {
    fn pick(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        self.u.choose_index(n).unwrap_or(0)
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.pick(one_in) == one_in - 1
    }

    /// A static text, sometimes empty.
    fn static_text(&mut self) -> String {
        TEXTS[self.pick(TEXTS.len())].to_owned()
    }

    fn text_part(&mut self, in_for: bool) -> TextPart {
        match self.pick(if in_for { 5 } else { 4 }) {
            0 | 3 => TextPart::Static(self.static_text()),
            1 => TextPart::Prop,
            2 => {
                let lit = TEXTS[self.pick(TEXTS.len())].to_owned();
                TextPart::Interp(lit)
            }
            _ => TextPart::Item,
        }
    }

    /// Children of a text-holding element (`paragraph`, `text`, `item`,
    /// `heading`, `button`, `emphasis`).
    fn phrasing(&mut self, depth: u32, in_for: bool) -> Vec<Node> {
        let n = 1 + self.pick(3);
        let mut out = Vec::new();
        for _ in 0..n {
            self.budget = self.budget.saturating_sub(1);
            if depth < 3 && self.budget > 0 && self.chance(4) {
                let name = ["text", "emphasis"][self.pick(2)].to_owned();
                let children = self.phrasing(depth + 1, in_for);
                out.push(Node::Element {
                    name,
                    label: false,
                    children,
                });
            } else {
                out.push(Node::Text(self.text_part(in_for)));
            }
        }
        out
    }

    fn cond(&mut self) -> String {
        match self.pick(4) {
            0 | 1 => "isOn".to_owned(),
            2 => "count > 2".to_owned(),
            _ => "title.isEmpty".to_owned(),
        }
    }

    /// Children of a container (`box`, `section`, `article`).
    fn flow(&mut self, depth: u32, in_for: bool) -> Vec<Node> {
        let n = 1 + self.pick(3);
        let mut out = Vec::new();
        for _ in 0..n {
            self.budget = self.budget.saturating_sub(2);
            let nested = depth < 4 && self.budget > 0;
            let choice = if nested { self.pick(8) } else { self.pick(3) };
            let node = match choice {
                0 => {
                    let name = ["paragraph", "heading(level: 2)", "button", "text"][self.pick(4)]
                        .to_owned();
                    Node::Element {
                        name,
                        label: false,
                        children: self.phrasing(depth + 1, in_for),
                    }
                }
                1 | 2 => Node::Text(self.text_part(in_for)),
                3 | 4 => {
                    let name = CONTAINERS[self.pick(CONTAINERS.len())].to_owned();
                    let label = name == "section" && self.chance(2);
                    Node::Element {
                        name,
                        label,
                        children: self.flow(depth + 1, in_for),
                    }
                }
                5 => self.if_node(depth, in_for),
                6 if !in_for => Node::For(self.phrasing(depth + 1, true)),
                _ => {
                    let n = 1 + self.pick(2);
                    let items = (0..n)
                        .map(|_| Node::Element {
                            name: "item".to_owned(),
                            label: false,
                            children: self.phrasing(depth + 2, in_for),
                        })
                        .collect();
                    Node::Element {
                        name: "list".to_owned(),
                        label: self.chance(3),
                        children: items,
                    }
                }
            };
            out.push(node);
        }
        out
    }

    /// `if cond { … } else { … }` over flow content: texts, elements and
    /// nested blocks at the top of a branch included.
    fn if_node(&mut self, depth: u32, in_for: bool) -> Node {
        let cond = self.cond();
        let then = self.flow(depth + 1, in_for);
        let otherwise = self.chance(2).then(|| self.flow(depth + 1, in_for));
        Node::If {
            cond,
            then,
            otherwise,
        }
    }

    fn props(&mut self) -> Props {
        let mut items: Vec<String> = Vec::new();
        for _ in 0..self.pick(4) {
            let t = TEXTS[self.pick(TEXTS.len())].to_owned();
            if !items.contains(&t) {
                items.push(t);
            }
        }
        Props {
            title: TEXTS[self.pick(TEXTS.len())].to_owned(),
            count: [0, 1, 3, -7, 9_007_199_254_740_991][self.pick(5)],
            is_on: self.chance(2),
            items,
        }
    }
}

fn write_nodes(out: &mut String, nodes: &[Node], level: usize) {
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_node(out, node, level);
    }
}

fn write_node(out: &mut String, node: &Node, level: usize) {
    let pad = "    ".repeat(level);
    match node {
        Node::Element {
            name,
            label,
            children,
        } => {
            let _ = writeln!(out, "{pad}{name} {{");
            write_nodes(out, children, level + 1);
            let _ = writeln!(out, "{pad}}}");
            if *label {
                let _ = writeln!(out, "{pad}.label(title)");
            }
        }
        Node::Text(part) => {
            let text = match part {
                TextPart::Static(s) => string_literal(s),
                TextPart::Prop => "title".to_owned(),
                TextPart::Interp(lit) => {
                    let lit = crate::exprgen::escape(lit);
                    format!("\"{lit} {{count}} {{title}} {{isOn}}\"")
                }
                TextPart::Item => "entry".to_owned(),
            };
            let _ = writeln!(out, "{pad}{text}");
        }
        Node::If {
            cond,
            then,
            otherwise,
        } => {
            let _ = writeln!(out, "{pad}if {cond} {{");
            write_nodes(out, then, level + 1);
            match otherwise {
                Some(o) => {
                    let _ = writeln!(out, "{pad}}} else {{");
                    write_nodes(out, o, level + 1);
                    let _ = writeln!(out, "{pad}}}");
                }
                None => {
                    let _ = writeln!(out, "{pad}}}");
                }
            }
        }
        Node::For(children) => {
            let _ = writeln!(out, "{pad}list {{");
            let _ = writeln!(out, "{pad}    for entry in items key entry {{");
            let _ = writeln!(out, "{pad}        item {{");
            write_nodes(out, children, level + 3);
            let _ = writeln!(out, "{pad}        }}");
            let _ = writeln!(out, "{pad}    }}");
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

/// The module of a generated case.
pub const MODULE: &str = "ui";

/// Generates one case from `u`.
pub fn generate(u: &mut Unstructured<'_>) -> Case {
    let mut g = Gen { u, budget: 60 };
    // The root is one element, or now and then a block (`if` at the top).
    let view = if g.chance(4) {
        vec![g.if_node(1, false)]
    } else {
        vec![Node::Element {
            name: "box".to_owned(),
            label: false,
            children: g.flow(1, false),
        }]
    };
    let first = g.props();
    let second = g.props();
    let mut body = String::new();
    write_nodes(&mut body, &view, 2);
    let source = format!(
        "public component Generated {{
    @Prop let title: String
    @Prop let count: Int
    @Prop let isOn: Bool
    @Prop let items: List<String>

    view {{
{body}    }}
}}
"
    );
    Case {
        view,
        first,
        second,
        files: vec![
            (
                "k.toml".to_owned(),
                "[package]\nname = \"gen\"\nk = \"0.1\"\n".to_owned(),
            ),
            (format!("src/{MODULE}/Generated.k"), source),
        ],
    }
}

fn props_json(p: &Props) -> String {
    serde_json::json!({
        "title": p.title,
        "count": p.count,
        "isOn": p.is_on,
        "items": p.items,
    })
    .to_string()
}

/// The driver: renders, parses, hydrates and updates, and returns every
/// HTML text it saw. `RUNTIME`, `CLIENT`, `FIRST` and `SECOND` are
/// replaced by the client runtime chunk, the chunk exporting the
/// component, and the two props as JSON.
const DRIVER: &str = r#"
import { installDom } from "k:testing";
import { batch, hydrate, signal } from RUNTIME;
import { Out } from "k:runtime/server";
import { Generated$ssr } from "k:/app/ui.js";
import { Generated } from CLIENT;

installDom();
const first = FIRST;
const second = SECOND;
const ctx = () => ({ c: {} });
const thunks = (p) => ({ title: () => p.title, count: () => p.count, isOn: () => p.isOn, items: () => p.items });
function signals(p) {
  const s = { title: signal(p.title), count: signal(p.count), isOn: signal(p.isOn), items: signal(p.items) };
  const props = { title: () => s.title.get(), count: () => s.count.get(), isOn: () => s.isOn.get(), items: () => s.items.get() };
  const set = (q) => batch(() => { s.title.set(q.title); s.count.set(q.count); s.isOn.set(q.isOn); s.items.set(q.items); });
  return { props, set };
}
function ssr(p) {
  const out = new Out();
  Generated$ssr(thunks(p), ctx(), out);
  return out.html();
}
function nodes(root) {
  const all = [];
  const walk = (n) => { for (const c of n.childNodes) { all.push(c); walk(c); } };
  walk(root);
  return all;
}
const report = {};
const html = ssr(first);
report.ssr = html;
const server = document.createElement("div");
server.innerHTML = html;
document.body.appendChild(server);
report.parsed = server.innerHTML;
const before = nodes(server);

const client = document.createElement("div");
document.body.appendChild(client);
const c = signals(first);
client.appendChild(Generated(c.props, ctx()));
report.client = client.innerHTML;

const h = signals(first);
hydrate(server, () => Generated(h.props, ctx()), null);
report.hydrated = server.innerHTML;
const after = nodes(server);
// Every server node is kept; the only nodes hydration may add are the
// empty text nodes the server can't send (an empty dynamic text).
const kept = new Set(after);
const describe = (n) => n.nodeType === 1 ? "<" + n.localName + ">" : n.nodeType === 3 ? "text " + JSON.stringify(n.data) : "comment " + JSON.stringify(n.data);
report.lost = before.filter((n) => !kept.has(n)).map(describe).join(", ");
report.added = after.filter((n) => !before.includes(n)).map(describe).join(", ");
report.claimed = before.every((n) => kept.has(n)) &&
  after.filter((n) => !before.includes(n)).every((n) => n.nodeType === 3 && n.data === "");

c.set(second);
h.set(second);
report.clientUpdated = client.innerHTML;
report.hydratedUpdated = server.innerHTML;
report.ssrSecond = ssr(second);
Deno.core.ops.op_k_set_result(JSON.stringify(report));
"#;

/// Checks one case (see the module documentation).
///
/// # Errors
///
/// A description of the first difference, with the project and the HTML.
pub fn check(case: &Case, limit: Duration) -> Result<(), String> {
    let output = coracle::check_codegen(
        &case.files,
        &|profile| Options {
            profile,
            ..Options::default()
        },
        &[],
    )?;
    let mut modules: Vec<JsModule> = coracle::server_modules(&output);
    let mut client = None;
    let mut runtime = None;
    for f in &output.client {
        if f.kind != FileKind::JavaScript {
            continue;
        }
        let specifier = format!("k:/client/{}", f.path);
        if f.path.starts_with("runtime.") {
            runtime = Some(specifier.clone());
        }
        if f.contents.contains("export function Generated(") {
            client = Some(specifier.clone());
        }
        modules.push(JsModule {
            specifier,
            source: f.contents.clone(),
        });
    }
    let Some(client) = client else {
        return Err(format!(
            "no client file exports the component\n{}",
            coracle::dump(&case.files)
        ));
    };
    if std::env::var_os("K_DIFF_DUMP").is_some() {
        for m in &modules {
            eprintln!("// ===== {}\n{}", m.specifier, m.source);
        }
    }
    // The client chunks import the client runtime chunk, a module instance
    // of its own: hydration state lives there, so the driver uses it too.
    let Some(runtime) = runtime else {
        return Err("the client bundle has no runtime chunk".to_owned());
    };
    let quoted = serde_json::to_string(&client).unwrap_or_default();
    let driver = DRIVER
        .replace(
            "RUNTIME",
            &serde_json::to_string(&runtime).unwrap_or_default(),
        )
        .replace("CLIENT", &quoted)
        .replace("FIRST", &props_json(&case.first))
        .replace("SECOND", &props_json(&case.second));
    let mut engine = Engine::new(EngineOptions {
        modules,
        timeout: Some(limit),
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    let fail = |what: String| {
        Err(format!(
            "{what}\n  first props: {:?}\n  second props: {:?}\n{}",
            case.first,
            case.second,
            coracle::dump(&case.files)
        ))
    };
    let report = match engine.run_main(&driver) {
        Ok(Some(text)) => text,
        Ok(None) => return fail("the driver produced no report".to_owned()),
        Err(e) => return fail(format!("the driver failed: {e}")),
    };
    let r: serde_json::Value = serde_json::from_str(&report).map_err(|e| e.to_string())?;
    // Two spellings of the same markup: the fake DOM serializes an empty
    // attribute as `name` (HTML: `name=""`) and doesn't escape `'` in an
    // attribute value (the server writes `&#39;`). No generated text
    // contains `=""` or `&#39;` itself.
    let s = |k: &str| {
        r[k].as_str()
            .unwrap_or("<missing>")
            .replace("=\"\"", "")
            .replace("&#39;", "'")
    };
    let ssr = s("ssr");
    if s("parsed") != ssr {
        return fail(format!(
            "the server HTML does not parse back to itself:\n  server: {ssr}\n  parsed: {}",
            s("parsed")
        ));
    }
    if s("client") != ssr {
        return fail(format!(
            "the client render differs from the server HTML:\n  server: {ssr}\n  client: {}",
            s("client")
        ));
    }
    if s("hydrated") != ssr {
        return fail(format!(
            "hydration changed the server HTML:\n  server:   {ssr}\n  hydrated: {}",
            s("hydrated")
        ));
    }
    if r["claimed"] != serde_json::Value::Bool(true) {
        return fail(format!(
            "hydration replaced server nodes instead of claiming them:\n  server: {ssr}\n  lost:   {}\n  added:  {}",
            s("lost"),
            s("added")
        ));
    }
    if s("hydratedUpdated") != s("clientUpdated") {
        return fail(format!(
            "after the update the hydrated DOM differs from the client DOM:\n  client:   {}\n  hydrated: {}",
            s("clientUpdated"),
            s("hydratedUpdated")
        ));
    }
    if s("clientUpdated") != s("ssrSecond") {
        return fail(format!(
            "after the update the DOM differs from a fresh render:\n  updated: {}\n  fresh:   {}",
            s("clientUpdated"),
            s("ssrSecond")
        ));
    }
    let _ = Profile::Development;
    Ok(())
}

//! Stability levels: deprecated declarations and preview features
//! (`docs/design/48-releases-and-versioning.md`, "Stability levels" and
//! "Deprecation"; `docs/compiler/release.md` §12, step 7).
//!
//! The pass runs once per revision, after every body is checked, over the
//! revision's resolutions (the same index the tooling queries read), so a
//! replayed file's uses are seen like a checked one's:
//!
//! * a use of a `@Deprecated` declaration or member is the warning K1401,
//!   with a machine-applicable fix when the `replacement` has the same
//!   shape ([`replacement_fix`]);
//! * a use of a standard-library declaration marked `@Preview("id")` is
//!   the error K8108 unless the package of the using file lists `id` in
//!   `[package] preview`, with a fix that adds it; a workspace library's
//!   files are checked against the library's own `k.toml`;
//! * each library mounted in the analysis whose `preview` lists a feature
//!   the project doesn't is K8109 in the project's `k.toml`;
//! * in the project's own `preview` list, an id the feature table doesn't
//!   know, or a planned one, is K8101, and a stable one is the warning
//!   K8110 with a fix that removes it.
//!
//! Annotations aren't resolutions (they are known by name), so annotation
//! uses (`@Worker`) are found from the declarations' annotation lists.
//! Imports and doc links name declarations without using them, and the
//! standard library's own files aren't checked: their uses are the
//! toolchain's business, never a project's warning. Without a `k.toml` in
//! the analysis (loose files) there is no package to opt in, and every
//! preview may be used.

use std::sync::Arc;

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextEdit, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, param_list_labels};
use crate::features::{self, Feature, Status};
use crate::fx::FxHashMap;
use crate::lookup::Resolution;
use crate::program::{DeclId, DeclKind, FileIx, MemberId, MemberKind};
use crate::warm::Chunked;

/// What `@Deprecated(…)` says.
#[derive(Clone, Debug, Default)]
struct Deprecation {
    /// The message.
    message: String,
    /// The version that deprecated it.
    since: String,
    /// The replacement's name, with labels for a function; empty for none.
    replacement: String,
}

/// The declaration or member a mark is on.
#[derive(Clone, Copy, Debug)]
enum Target {
    /// A top-level declaration.
    Decl(DeclId),
    /// A member.
    Member(MemberId),
}

/// The stability annotations of one declaration or member.
#[derive(Clone, Debug, Default)]
struct Mark {
    /// `@Deprecated(…)`.
    deprecated: Option<Deprecation>,
    /// `@Preview("id")`'s id.
    preview: Option<String>,
    /// What carries the mark; `None` only in [`Mark::default`].
    target: Option<Target>,
}

/// One package's `[package] preview` list.
struct PreviewList {
    /// The manifest's file id.
    file: FileId,
    /// The manifest's text.
    text: Arc<str>,
    /// `[package] name`.
    name: String,
    /// The listed ids, with the range of each string (quotes included).
    ids: Vec<(String, TextRange)>,
}

/// Runs the pass (see the module documentation).
pub(crate) fn check(cx: &Cx<'_, '_>, resolutions: &Chunked<Resolution>) {
    let p = cx.p;
    let manifests = manifests(cx);
    // The project's own list, first: unknown, planned and stable ids.
    if let Some(own) = manifests.get("") {
        own_list_problems(cx, own, features::FEATURES);
    }
    dependency_previews(cx, &manifests);
    language_uses(cx, &manifests);
    // Every marked declaration and member.
    let mut marks: FxHashMap<Resolution, Mark> = FxHashMap::default();
    for (i, d) in p.decls.iter().enumerate() {
        if !is_marked(&d.annotations) {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        marks.insert(cx.decl_resolution(id), mark_of(&d.node, Target::Decl(id)));
    }
    for (i, m) in p.members.iter().enumerate() {
        let Some(node) = m.node.as_ref().filter(|_| is_marked(&m.annotations)) else {
            continue;
        };
        let id = MemberId(u32::try_from(i).unwrap_or(u32::MAX));
        marks.insert(cx.member_resolution(id), mark_of(node, Target::Member(id)));
    }
    if marks.is_empty() {
        return;
    }
    // Few declarations are marked: comparing each resolution with each of
    // them (lengths first) is cheaper than hashing every resolution's
    // strings, and this loop sees every resolution of the revision.
    let few: Vec<(&Resolution, &Mark)> = marks.iter().collect();
    let lookup = |r: &Resolution| -> Option<&Mark> {
        if few.len() > 16 {
            return marks.get(r);
        }
        few.iter().find(|(m, _)| same(m, r)).map(|(_, mark)| *mark)
    };
    let mut found: Vec<Diagnostic> = Vec::new();
    for chunk in resolutions.chunks() {
        for ((file, range), resolution) in chunk.iter() {
            let Some(mark) = lookup(resolution) else {
                continue;
            };
            let Some(&ix) = p.file_by_id.get(file) else {
                continue;
            };
            if p.file(ix).stdlib
                || !is_use(&p.file(ix).root, *range)
                || within(cx, mark, ix, *range)
            {
                continue;
            }
            report_use(cx, &manifests, mark, ix, *range, resolution, &mut found);
        }
    }
    annotation_uses(cx, &manifests, &marks, &mut found);
    found.sort_by(|a, b| (a.span, a.code, &a.message).cmp(&(b.span, b.code, &b.message)));
    found.dedup();
    cx.diags.borrow_mut().extend(found);
}

/// `true` when `a` and `b` are the same declaration or member, comparing
/// the cheap parts first.
fn same(a: &Resolution, b: &Resolution) -> bool {
    match (a, b) {
        (Resolution::Declaration { name: x }, Resolution::Declaration { name: y }) => {
            x.len() == y.len() && x == y
        }
        (
            Resolution::Member {
                owner: o1,
                name: n1,
            },
            Resolution::Member {
                owner: o2,
                name: n2,
            },
        ) => n1.len() == n2.len() && o1.len() == o2.len() && n1 == n2 && o1 == o2,
        (Resolution::Declaration { .. } | Resolution::Member { .. }, _)
        | (_, Resolution::Declaration { .. } | Resolution::Member { .. }) => false,
        _ => a == b,
    }
}

/// `true` when the annotation names include a stability annotation.
fn is_marked(annotations: &[String]) -> bool {
    annotations
        .iter()
        .any(|a| a == "Deprecated" || a == "Preview")
}

/// Reads the stability annotations directly on `node`.
fn mark_of(node: &SyntaxNode, target: Target) -> Mark {
    let mut mark = Mark {
        target: Some(target),
        ..Mark::default()
    };
    for a in node.children().filter_map(ast::Annotation::cast) {
        let Some(name) = a.name() else { continue };
        let args: Vec<(Option<String>, String)> = a
            .arg_list()
            .map(|list| {
                list.args()
                    .map(|arg| {
                        let label = arg.label().map(|l| l.text().to_owned());
                        let value = arg
                            .value()
                            .and_then(|v| plain_string(v.syntax()))
                            .unwrap_or_default();
                        (label, value)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let arg = |label: Option<&str>| {
            args.iter()
                .find(|(l, _)| l.as_deref() == label)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        match name.text() {
            "Deprecated" => {
                mark.deprecated = Some(Deprecation {
                    message: arg(None),
                    since: arg(Some("since")),
                    replacement: arg(Some("replacement")),
                });
            }
            "Preview" => mark.preview = Some(arg(None)).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    mark
}

/// The text of a string literal without interpolations or escapes.
fn plain_string(node: &SyntaxNode) -> Option<String> {
    let lit = std::iter::once(node.clone())
        .chain(node.descendants())
        .find(|n| n.kind() == SyntaxKind::StringLit)?;
    let mut text = String::new();
    for child in lit.children_with_tokens() {
        let token = child.into_token()?;
        match token.kind() {
            SyntaxKind::StringStart | SyntaxKind::StringEnd => {}
            SyntaxKind::StringText => text.push_str(token.text()),
            _ => return None,
        }
    }
    Some(text)
}

/// `true` when the reference at `range` is a use: not in a comment (a doc
/// link) and not in an import.
fn is_use(root: &SyntaxNode, range: TextRange) -> bool {
    let Some(token) = root.token_at_offset(range.start()).right_biased() else {
        return true;
    };
    if matches!(
        token.kind(),
        SyntaxKind::DocComment | SyntaxKind::LineComment | SyntaxKind::BlockComment
    ) {
        return false;
    }
    !token
        .parent_ancestors()
        .any(|n| matches!(n.kind(), SyntaxKind::Import | SyntaxKind::ImportPath))
}

/// `true` when the reference is inside the marked declaration itself (its
/// own name, or a body that calls itself or its sibling).
fn within(cx: &Cx<'_, '_>, mark: &Mark, file: FileIx, range: TextRange) -> bool {
    let p = cx.p;
    let (decl_file, node) = match mark.target {
        Some(Target::Decl(d)) => (p.decl(d).file, Some(p.decl(d).node.clone())),
        Some(Target::Member(m)) => (p.decl(p.member(m).owner).file, p.member(m).node.clone()),
        None => return false,
    };
    decl_file == file && node.is_some_and(|n| n.text_range().contains_range(range))
}

/// The display name of a mark's target: `sum(of:)`, `Worker`.
fn shown_name(resolution: &Resolution) -> String {
    match resolution {
        Resolution::Declaration { name } => name.rsplit('.').next().unwrap_or(name).to_owned(),
        Resolution::Member { owner, name } => {
            format!("{}.{name}", owner.rsplit('.').next().unwrap_or(owner))
        }
        _ => String::new(),
    }
}

/// Reports one use of a marked declaration.
fn report_use(
    cx: &Cx<'_, '_>,
    manifests: &FxHashMap<String, PreviewList>,
    mark: &Mark,
    file: FileIx,
    range: TextRange,
    resolution: &Resolution,
    out: &mut Vec<Diagnostic>,
) {
    let span = cx.p.span(file, range);
    if let Some(dep) = &mark.deprecated {
        out.push(deprecated(cx, dep, mark, file, span, resolution));
    }
    if let Some(id) = &mark.preview
        && let Some(d) = preview_use(cx, manifests, id, file, span, &shown_name(resolution))
    {
        out.push(d);
    }
}

/// K1401 for a use at `span`.
fn deprecated(
    cx: &Cx<'_, '_>,
    dep: &Deprecation,
    mark: &Mark,
    file: FileIx,
    span: Span,
    resolution: &Resolution,
) -> Diagnostic {
    let name = shown_name(resolution);
    let since = if dep.since.is_empty() {
        String::new()
    } else {
        format!(" since {}", dep.since)
    };
    let message = if dep.message.is_empty() {
        format!("`{name}` is deprecated{since}")
    } else {
        format!("`{name}` is deprecated{since}: {}", dep.message)
    };
    let mut d = Diagnostic::new(codes::DEPRECATED, span, message);
    match replacement_fix(cx, dep, mark, file, span) {
        Some(fix) => d = d.with_fix(fix),
        None if !dep.replacement.is_empty() => {
            d = d.with_help(format!("use `{}` instead", dep.replacement));
        }
        None => {}
    }
    d
}

/// The argument labels of a function's node: `(of:)`, `(_:to:)`; `None`
/// for anything that isn't a function.
fn labels_of(node: &SyntaxNode, is_func: bool) -> Option<String> {
    is_func.then(|| {
        let mut out = String::from("(");
        for label in param_list_labels(node) {
            out.push_str(label.as_deref().unwrap_or("_"));
            out.push(':');
        }
        out.push(')');
        out
    })
}

/// The fix of K1401: the use's name replaced by the replacement's, when the
/// replacement is a declaration of the same kind beside the deprecated one
/// (a member of the same type, or a declaration of the same module) with
/// the same argument labels, and the use sees it without a new import.
fn replacement_fix(
    cx: &Cx<'_, '_>,
    dep: &Deprecation,
    mark: &Mark,
    file: FileIx,
    span: Span,
) -> Option<Fix> {
    let p = cx.p;
    let (base, written_labels) = match dep.replacement.find('(') {
        Some(i) => (&dep.replacement[..i], Some(&dep.replacement[i..])),
        None => (dep.replacement.as_str(), None),
    };
    if base.is_empty() || !base.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let text = p.file(file).text;
    let written = text.get(usize::from(span.range.start())..usize::from(span.range.end()))?;
    let same_labels = |deprecated: &Option<String>, replacement: &Option<String>| {
        deprecated == replacement && written_labels.is_none_or(|w| Some(w) == deprecated.as_deref())
    };
    match mark.target? {
        Target::Member(m) => {
            let member = p.member(m);
            if written != member.name {
                return None;
            }
            let is_func = |k: MemberKind| matches!(k, MemberKind::Func | MemberKind::Init);
            let old = labels_of(member.node.as_ref()?, is_func(member.kind));
            let owner = p.decl(member.owner);
            let found = owner.members.iter().any(|&o| {
                let other = p.member(o);
                other.name == base
                    && other.kind == member.kind
                    && other.is_static == member.is_static
                    && other
                        .node
                        .as_ref()
                        .is_some_and(|n| same_labels(&old, &labels_of(n, is_func(other.kind))))
            });
            found.then(|| Fix::replace(format!("Replace with `{base}`"), span, base))
        }
        Target::Decl(d) => {
            let decl = p.decl(d);
            if written != decl.name {
                return None;
            }
            let module = p.decl_module(d)?;
            let is_func = decl.kind == DeclKind::Named(crate::stdlib::DeclarationKind::Function);
            let old = labels_of(&decl.node, is_func);
            let other = p.modules[module.0 as usize]
                .names
                .get(base)
                .and_then(|list| list.first().copied())?;
            let o = p.decl(other);
            if o.kind != decl.kind || !same_labels(&old, &labels_of(&o.node, is_func)) {
                return None;
            }
            let module_path = &p.modules[module.0 as usize].path;
            let visible = p.file(file).module == Some(module)
                || (module_path == crate::stdlib::CORE_MODULE
                    && o.visibility == ast::Visibility::Public);
            visible.then(|| Fix::replace(format!("Replace with `{base}`"), span, base))
        }
    }
}

/// The manifest that governs `file`: its package's `k.toml`, the project's
/// own (`""`) or a mounted library's (`.deps/<dir>`).
fn package_of(cx: &Cx<'_, '_>, file: FileIx) -> String {
    cx.p.file(file)
        .path
        .and_then(|path| {
            let rest = path.as_str().strip_prefix(crate::DEPENDENCY_DIR)?;
            let segment = rest.strip_prefix('/')?.split('/').next()?;
            Some(format!("{}/{segment}", crate::DEPENDENCY_DIR))
        })
        .unwrap_or_default()
}

/// K8108 for a use at `span` of a declaration of preview feature `id`,
/// unless the file's package lists it.
fn preview_use(
    cx: &Cx<'_, '_>,
    manifests: &FxHashMap<String, PreviewList>,
    id: &str,
    file: FileIx,
    span: Span,
    name: &str,
) -> Option<Diagnostic> {
    // Without a manifest (loose files) every preview may be used.
    let list = manifests.get(&package_of(cx, file))?;
    if list.ids.iter().any(|(i, _)| i == id) {
        return None;
    }
    let mut d = Diagnostic::new(
        codes::PREVIEW_NOT_ENABLED,
        span,
        format!("`{name}` is part of the preview feature `{id}`, which `{}` doesn't list in `[package] preview`", list.name),
    )
    .with_note(
        "a preview may change or go away in any minor release; listing it in k.toml is the consent",
    );
    if let Some(edit) = add_feature(list, id) {
        d = d.with_fix(Fix {
            message: format!("Add `\"{id}\"` to `[package] preview`"),
            edits: vec![edit],
            applicability: k_diagnostics::Applicability::MachineApplicable,
        });
    }
    Some(d)
}

/// The language features in preview: K8108 at each use of their syntax,
/// unless the file's package lists them. A `preview` declaration is the
/// feature `view-previews` (docs/design/57-previews.md), reported at its
/// keyword.
fn language_uses(cx: &Cx<'_, '_>, manifests: &FxHashMap<String, PreviewList>) {
    let p = cx.p;
    let mut found = Vec::new();
    for d in &p.decls {
        if d.named_kind() != Some(crate::stdlib::DeclarationKind::Preview) || p.file(d.file).stdlib
        {
            continue;
        }
        let span = p.span(d.file, d.name_range);
        if let Some(diag) = preview_use(cx, manifests, VIEW_PREVIEWS, d.file, span, "preview") {
            found.push(diag);
        }
    }
    cx.diags.borrow_mut().extend(found);
}

/// The feature id of `preview` declarations.
pub(crate) const VIEW_PREVIEWS: &str = "view-previews";

/// The annotation uses of marked annotation declarations (`@Worker`),
/// which aren't resolutions.
fn annotation_uses(
    cx: &Cx<'_, '_>,
    manifests: &FxHashMap<String, PreviewList>,
    marks: &FxHashMap<Resolution, Mark>,
    out: &mut Vec<Diagnostic>,
) {
    let p = cx.p;
    // The marked annotation types, by simple name.
    let mut by_name: FxHashMap<&str, DeclId> = FxHashMap::default();
    for mark in marks.values() {
        if let Some(Target::Decl(d)) = mark.target
            && p.decl(d).has_annotation("Annotation")
        {
            by_name.insert(p.decl(d).name.as_str(), d);
        }
    }
    if by_name.is_empty() {
        return;
    }
    let nodes = p
        .decls
        .iter()
        .map(|d| (d.file, Some(&d.node), &d.annotations))
        .chain(
            p.members
                .iter()
                .map(|m| (p.decl(m.owner).file, m.node.as_ref(), &m.annotations)),
        );
    for (file, node, names) in nodes {
        if p.file(file).stdlib || !names.iter().any(|n| by_name.contains_key(n.as_str())) {
            continue;
        }
        let Some(node) = node else { continue };
        for a in crate::framework::annotations_of(cx, file, node) {
            if !by_name.values().any(|d| *d == a.decl) {
                continue;
            }
            let resolution = cx.decl_resolution(a.decl);
            let Some(mark) = marks.get(&resolution) else {
                continue;
            };
            // The span is `@Name`, as K3002 reports annotations.
            report_use(cx, manifests, mark, file, a.range, &resolution, out);
        }
    }
}

// ------------------------------------------------------------ manifests

/// The manifests of the analysis with their preview lists: the project's
/// own under `""`, each mounted library's under `.deps/<dir>`.
fn manifests(cx: &Cx<'_, '_>) -> FxHashMap<String, PreviewList> {
    let mut out = FxHashMap::default();
    for (path, text) in &cx.p.manifests {
        let Some(&file) = cx.p.manifest_ids.get(path) else {
            continue;
        };
        let key = match path.strip_suffix("k.toml") {
            Some("") => String::new(),
            Some(dir) => dir.trim_end_matches('/').to_owned(),
            None => continue,
        };
        let Ok(table) = toml::from_str::<toml::Table>(text) else {
            continue;
        };
        let package = table.get("package").and_then(toml::Value::as_table);
        let name = package
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("this package")
            .to_owned();
        let ids = package
            .and_then(|p| p.get("preview"))
            .and_then(toml::Value::as_array)
            .map(|list| {
                let mut from = preview_key(text).map_or(0, |r| usize::from(r.start()));
                list.iter()
                    .filter_map(toml::Value::as_str)
                    .map(|id| {
                        let range = string_range(text, id, &mut from).unwrap_or_default();
                        (id.to_owned(), range)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.insert(
            key,
            PreviewList {
                file,
                text: Arc::clone(text),
                name,
                ids,
            },
        );
    }
    out
}

/// The range of the `preview` key's line in `[package]`.
fn preview_key(text: &str) -> Option<TextRange> {
    let (start, end) = section(text, "package")?;
    let mut offset = start;
    for line in text[start..end].split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("preview")
            && trimmed["preview".len()..].trim_start().starts_with('=')
        {
            return Some(range(offset, offset + line.trim_end().len()));
        }
        offset += line.len();
    }
    None
}

/// The byte range of the body of `[name]` (after its header line, up to the
/// next header or the end).
fn section(text: &str, name: &str) -> Option<(usize, usize)> {
    let header = format!("[{name}]");
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if start.is_some() && trimmed.starts_with('[') {
            return start.map(|s| (s, offset));
        }
        if trimmed == header {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start.map(|s| (s, text.len()))
}

/// A range from byte offsets.
fn range(start: usize, end: usize) -> TextRange {
    let size = |n: usize| TextSize::from(u32::try_from(n).unwrap_or(u32::MAX));
    TextRange::new(size(start), size(end.max(start)))
}

/// The range of the next `"value"` at or after `from`, advancing `from`.
fn string_range(text: &str, value: &str, from: &mut usize) -> Option<TextRange> {
    let quoted = format!("\"{value}\"");
    let at = *from + text.get(*from..)?.find(&quoted)?;
    *from = at + quoted.len();
    Some(range(at, at + quoted.len()))
}

/// The edit that adds `id` to the list's `[package] preview`: into the
/// array when the key exists, else as a new line after the section's last
/// line.
fn add_feature(list: &PreviewList, id: &str) -> Option<TextEdit> {
    let text = &*list.text;
    let edit = |at: usize, replacement: String| TextEdit {
        span: Span::new(list.file, range(at, at)),
        replacement,
    };
    if let Some(key) = preview_key(text) {
        let start = usize::from(key.start());
        let close = start + text[start..].find(']')?;
        let open = start + text[start..close].find('[')?;
        let inside = text[open + 1..close].trim();
        return Some(if inside.is_empty() {
            edit(open + 1, format!("\"{id}\""))
        } else {
            let end = open + 1 + text[open + 1..close].trim_end().len();
            let comma = if inside.ends_with(',') { " " } else { ", " };
            edit(end, format!("{comma}\"{id}\""))
        });
    }
    let (start, end) = section(text, "package")?;
    // After the last non-blank line of the section.
    let body = &text[start..end];
    let last = body.trim_end().len();
    let at = start + last;
    let (prefix, suffix) = if last == 0 { ("", "\n") } else { ("\n", "") };
    let newline = if at == text.len() && last > 0 {
        "\n"
    } else {
        suffix
    };
    Some(edit(at, format!("{prefix}preview = [\"{id}\"]{newline}")))
}

/// K8101 for unknown and planned ids in the project's own `preview`, and
/// K8110 for stable ones; `table` is the feature table.
fn own_list_problems(cx: &Cx<'_, '_>, own: &PreviewList, table: &[Feature]) {
    for d in list_problems(own, table) {
        cx.diags.borrow_mut().push(d);
    }
    // `preview` that isn't a list of strings.
    let text = &*own.text;
    if let Ok(table) = toml::from_str::<toml::Table>(text)
        && let Some(value) = table.get("package").and_then(|p| p.get("preview"))
        && !value
            .as_array()
            .is_some_and(|a| a.iter().all(toml::Value::is_str))
    {
        let at = preview_key(text).unwrap_or_default();
        cx.diags.borrow_mut().push(Diagnostic::new(
            codes::INVALID_MANIFEST,
            Span::new(own.file, at),
            "`[package] preview` must be a list of feature ids, such as [\"workers\"]",
        ));
    }
}

/// The problems of one preview list against `table` (see
/// [`own_list_problems`]).
fn list_problems(own: &PreviewList, table: &[Feature]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (id, at) in &own.ids {
        let span = Span::new(own.file, *at);
        match features::lookup_in(table, id) {
            None => out.push(
                Diagnostic::new(
                    codes::INVALID_MANIFEST,
                    span,
                    format!("`{id}` is not a preview feature"),
                )
                .with_note(preview_note(table)),
            ),
            Some(f) if f.status == Status::Planned => out.push(
                Diagnostic::new(
                    codes::INVALID_MANIFEST,
                    span,
                    format!("`{id}` is planned, not built: there is nothing to opt in to yet"),
                )
                .with_note(preview_note(table)),
            ),
            Some(f) if f.status == Status::Stable => {
                let mut d = Diagnostic::new(
                    codes::STABILIZED_PREVIEW,
                    span,
                    format!(
                        "`{id}` is stable since {}: it needs no opt-in",
                        f.since.unwrap_or("this version")
                    ),
                );
                if let Some(edit) = remove_feature(own, *at) {
                    d = d.with_fix(Fix {
                        message: format!("Remove `\"{id}\"`"),
                        edits: vec![edit],
                        applicability: k_diagnostics::Applicability::MachineApplicable,
                    });
                }
                out.push(d);
            }
            Some(_) => {}
        }
    }
    out
}

/// The note naming the features a project can opt in to.
fn preview_note(table: &[Feature]) -> String {
    let ids: Vec<String> = table
        .iter()
        .filter(|f| f.status == Status::Preview)
        .map(|f| format!("`{}`", f.id))
        .collect();
    if ids.is_empty() {
        "no feature is in preview".to_owned()
    } else {
        format!("the features in preview: {}", ids.join(", "))
    }
}

/// The edit that removes the string at `at` from the `preview` array, with
/// its comma, or the whole line when it is the only entry.
fn remove_feature(list: &PreviewList, at: TextRange) -> Option<TextEdit> {
    let text = &*list.text;
    let (s, e) = (usize::from(at.start()), usize::from(at.end()));
    let edit = |a: usize, b: usize| TextEdit {
        span: Span::new(list.file, range(a, b)),
        replacement: String::new(),
    };
    if list.ids.len() == 1 {
        let key = preview_key(text)?;
        let start = usize::from(key.start());
        let close = start + text[start..].find(']')?;
        let line_end = text[close..]
            .find('\n')
            .map_or(text.len(), |n| close + n + 1);
        return Some(edit(start, line_end));
    }
    // `"a", "b"`: take the following `, ` (or the preceding one for the last).
    let after = &text[e..];
    let gap = after.len() - after.trim_start_matches([',', ' ']).len();
    if after.starts_with(',') {
        return Some(edit(s, e + gap));
    }
    let before = &text[..s];
    let back = before.len() - before.trim_end_matches([',', ' ']).len();
    Some(edit(s - back, e))
}

/// K8109: each library mounted in the analysis whose `preview` lists a
/// preview feature the project's own list doesn't.
fn dependency_previews(cx: &Cx<'_, '_>, manifests: &FxHashMap<String, PreviewList>) {
    let Some(own) = manifests.get("") else { return };
    let mut libraries: Vec<&PreviewList> = manifests
        .iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(_, v)| v)
        .collect();
    libraries.sort_by(|a, b| a.name.cmp(&b.name));
    for lib in libraries {
        for (id, _) in &lib.ids {
            let is_preview = features::lookup(id).is_some_and(|f| f.status == Status::Preview);
            if !is_preview || own.ids.iter().any(|(i, _)| i == id) {
                continue;
            }
            let at = dependency_line(&own.text, &lib.name);
            let mut d = Diagnostic::new(
                codes::DEPENDENCY_PREVIEW,
                Span::new(own.file, at),
                format!(
                    "`{}` uses the preview feature `{id}`, so `{}` must list it in `[package] preview` too",
                    lib.name, own.name
                ),
            )
            .with_note("consent to a preview isn't inherited: every project that depends on its users lists it");
            if let Some(edit) = add_feature(own, id) {
                d = d.with_fix(Fix {
                    message: format!("Add `\"{id}\"` to `[package] preview`"),
                    edits: vec![edit],
                    applicability: k_diagnostics::Applicability::MachineApplicable,
                });
            }
            cx.diags.borrow_mut().push(d);
        }
    }
}

/// The `[dependencies]` line naming `name`, else the `[dependencies]`
/// header, else the start of the file.
fn dependency_line(text: &str, name: &str) -> TextRange {
    let mut offset = 0;
    let mut in_deps = false;
    let mut header = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_deps = trimmed == "[dependencies]";
            if in_deps {
                header = Some(range(offset, offset + line.trim_end().len()));
            }
        } else if in_deps {
            let key = trimmed
                .split('=')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches('"');
            if key == name {
                return range(offset, offset + line.trim_end().len());
            }
        }
        offset += line.len();
    }
    header.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(text: &str, ids: &[&str]) -> PreviewList {
        let mut from = preview_key(text).map_or(0, |r| usize::from(r.start()));
        PreviewList {
            file: FileId::new(7),
            text: Arc::from(text),
            name: "shop".to_owned(),
            ids: ids
                .iter()
                .map(|id| ((*id).to_owned(), string_range(text, id, &mut from).unwrap()))
                .collect(),
        }
    }

    fn applied(text: &str, edit: &TextEdit) -> String {
        k_diagnostics::fix::apply(text, std::slice::from_ref(edit))
    }

    /// No feature is stable yet, so the stable row is a test table's.
    const TABLE: &[Feature] = &[
        Feature {
            id: "old-thing",
            status: Status::Stable,
            since: Some("0.2"),
            summary: "",
            design: "",
        },
        Feature {
            id: "new-thing",
            status: Status::Preview,
            since: Some("0.2"),
            summary: "",
            design: "",
        },
        Feature {
            id: "later",
            status: Status::Planned,
            since: None,
            summary: "",
            design: "",
        },
    ];

    #[test]
    fn a_stabilized_feature_still_listed_is_a_warning_with_a_fix() {
        let text =
            "[package]\nname = \"shop\"\nk = \"0.2\"\npreview = [\"old-thing\", \"new-thing\"]\n";
        let l = list(text, &["old-thing", "new-thing"]);
        let found = list_problems(&l, TABLE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, codes::STABILIZED_PREVIEW);
        assert_eq!(
            found[0].message,
            "`old-thing` is stable since 0.2: it needs no opt-in"
        );
        let fixed = applied(text, &found[0].fixes[0].edits[0]);
        assert!(fixed.ends_with("preview = [\"new-thing\"]\n"), "{fixed}");
        // The only entry takes its line with it.
        let text = "[package]\nname = \"shop\"\npreview = [\"old-thing\"]\nk = \"0.2\"\n";
        let l = list(text, &["old-thing"]);
        let found = list_problems(&l, TABLE);
        assert_eq!(
            applied(text, &found[0].fixes[0].edits[0]),
            "[package]\nname = \"shop\"\nk = \"0.2\"\n"
        );
        // The last of several takes the comma before it.
        let text = "[package]\npreview = [\"new-thing\", \"old-thing\"]\n";
        let l = list(text, &["new-thing", "old-thing"]);
        let found = list_problems(&l, TABLE);
        assert_eq!(
            applied(text, &found[0].fixes[0].edits[0]),
            "[package]\npreview = [\"new-thing\"]\n"
        );
    }

    #[test]
    fn unknown_and_planned_ids_are_k8101() {
        let text = "[package]\npreview = [\"nope\", \"later\"]\n";
        let found = list_problems(&list(text, &["nope", "later"]), TABLE);
        let codes: Vec<_> = found.iter().map(|d| d.code).collect();
        assert_eq!(codes, [codes::INVALID_MANIFEST, codes::INVALID_MANIFEST]);
        assert_eq!(found[1].notes, ["the features in preview: `new-thing`"]);
    }

    #[test]
    fn adding_a_feature_creates_or_extends_the_list() {
        let text = "[package]\nname = \"shop\"\nk = \"0.1\"\n\n[dependencies]\nlib = { workspace = true }\n";
        let edit = add_feature(&list(text, &[]), "workers").unwrap();
        assert_eq!(
            applied(text, &edit),
            "[package]\nname = \"shop\"\nk = \"0.1\"\npreview = [\"workers\"]\n\n[dependencies]\nlib = { workspace = true }\n"
        );
        let text = "[package]\nname = \"shop\"\npreview = [\"a\"]\n";
        let edit = add_feature(&list(text, &["a"]), "workers").unwrap();
        assert_eq!(
            applied(text, &edit),
            "[package]\nname = \"shop\"\npreview = [\"a\", \"workers\"]\n"
        );
        let text = "[package]\npreview = []\n";
        let edit = add_feature(&list(text, &[]), "workers").unwrap();
        assert_eq!(applied(text, &edit), "[package]\npreview = [\"workers\"]\n");
        let text = "[package]\nname = \"shop\"";
        let edit = add_feature(&list(text, &[]), "workers").unwrap();
        assert_eq!(
            applied(text, &edit),
            "[package]\nname = \"shop\"\npreview = [\"workers\"]\n"
        );
    }
}

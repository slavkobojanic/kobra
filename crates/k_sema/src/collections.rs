//! Indexed collections (`docs/design/52-indexed-collections.md`): the
//! program's `Collection<Key, Value>` and `CollectionIndex<Field, Value>`
//! constants and fields, their checks and the schema the browser upgrades
//! its database to.
//!
//! * **K2403**: a `name:` or `collection:` that isn't a string literal;
//! * **K2410**: a collection named like an earlier one (in file path order)
//!   of the app's non-test code, or an index on the same collection and
//!   field as an earlier one, at its name literal with a label at the
//!   earlier one's;
//! * **K2411**: a `key:` or `field:` that isn't a string literal, or that
//!   doesn't name a stored field of the value type (a struct) whose type is
//!   the declared key or field type (`Field?` too for an index), or whose
//!   type isn't a key type (`String`, `Int`, `Uuid`, `Date`, an enum
//!   without payloads);
//! * **K2412**: an index naming a collection that no `Collection` of the
//!   same value type declares.
//!
//! Test code may redeclare a collection or an index to inspect what a
//! service stored, so it isn't counted for K2410; its collections join the
//! schema of a test build. Types that don't resolve aren't checked, and the
//! diagnostics resolving them causes are dropped (as `locks::program_keys`
//! drops them).

use std::collections::BTreeMap;

use k_diagnostics::{Diagnostic, TextRange, codes};
use k_syntax::SyntaxNode;
use k_syntax::ast::{self, AstNode};

use crate::cx::Cx;
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// How a key field's value becomes an IndexedDB key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// Text as it is encoded: `String`, `Uuid`, a payload-free enum's case
    /// name.
    Text,
    /// An `Int`, a number.
    Int,
    /// A `Date`, parsed from its RFC 3339 text to epoch milliseconds.
    Date,
}

impl KeyKind {
    /// The kind's name as the runtime's schema spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KeyKind::Text => "text",
            KeyKind::Int => "int",
            KeyKind::Date => "date",
        }
    }
}

/// A key field or an indexed field of a collection's value type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CollectionField {
    /// The field's K name (also the index's name).
    pub field: String,
    /// The key of the field in the value's JSON form (its `@JsonKey`, else
    /// its name).
    pub json: String,
    /// How its value becomes an IndexedDB key.
    pub kind: KeyKind,
}

/// One collection of the program, as the browser's database holds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CollectionSchema {
    /// The collection's name, the object store's.
    pub name: String,
    /// The key field.
    pub key: CollectionField,
    /// The indexed fields of the app's non-test code, sorted by field name.
    pub indexes: Vec<CollectionField>,
    /// The fields only test code indexes, sorted: a test build's database
    /// has them too.
    pub test_indexes: Vec<CollectionField>,
    /// Every stored field of the value whose type is a key type (or one
    /// made optional), in declaration order: each record carries their
    /// keys, so an index another build adds finds the values this build
    /// writes.
    pub fields: Vec<CollectionField>,
    /// `true` when only test code declares it.
    pub test: bool,
}

/// Which of the two declarations a [`Declared`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Which {
    /// A `Collection<Key, Value>`.
    Collection,
    /// A `CollectionIndex<Field, Value>`.
    Index,
}

/// A string-literal argument of an initializer: its text and range, or the
/// range of the expression that isn't one.
pub(crate) type LiteralArg = Result<(String, TextRange), TextRange>;

/// One `Collection` or `CollectionIndex` constant or field.
pub(crate) struct Declared {
    /// Which it is.
    pub(crate) which: Which,
    /// The qualified name of the constant or field.
    pub(crate) declaration: String,
    /// The `name:` (collection) or `collection:` (index) literal.
    pub(crate) name: Option<LiteralArg>,
    /// The `key:` (collection) or `field:` (index) literal.
    pub(crate) field: Option<LiteralArg>,
    /// The key or field type argument.
    pub(crate) first: Ty,
    /// The value type argument.
    pub(crate) value: Ty,
    /// The file declaring it.
    pub(crate) file: FileIx,
    /// The range of the constant's or field's name.
    pub(crate) range: TextRange,
    /// `true` in test code.
    pub(crate) test: bool,
    /// `true` when marked `@Breaking("reason")`.
    pub(crate) breaking: bool,
}

/// The type arguments of a `Collection` or `CollectionIndex` type.
fn declared_type(cx: &Cx<'_, '_>, ty: &Ty) -> Option<(Which, Ty, Ty)> {
    let Ty::Named(d, args) = ty else {
        return None;
    };
    let which = match cx.p.decl_name(*d).as_str() {
        "k.storage.Collection" => Which::Collection,
        "k.storage.CollectionIndex" => Which::Index,
        _ => return None,
    };
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Ty::Error);
    Some((which, arg(0), arg(1)))
}

/// The argument labelled `label` of the first call below `node`.
fn literal_arg(node: &SyntaxNode, label: &str) -> Option<LiteralArg> {
    let call = node.descendants().find_map(ast::CallExpr::cast)?;
    call.arg_list()?.args().find_map(|a| {
        if a.label()?.text() != label {
            return None;
        }
        let value = a.value()?;
        let range = value.syntax().text_range();
        Some(match &value {
            ast::Expr::StringLit(s) => s.plain_value().map(|t| (t, range)).ok_or(range),
            _ => Err(range),
        })
    })
}

/// `true` when `node` carries `@Breaking`.
fn has_breaking(node: &SyntaxNode) -> bool {
    node.children()
        .filter_map(ast::Annotation::cast)
        .any(|a| a.name().is_some_and(|t| t.text() == "Breaking"))
}

/// Every `Collection` and `CollectionIndex` of the program, test code
/// included, in file path order. Resolving their types keeps the
/// diagnostics that causes; [`declarations`] drops them.
fn find(cx: &Cx<'_, '_>) -> Vec<Declared> {
    let p = cx.p;
    let mut out = Vec::new();
    let mut push = |ty: &Ty, declaration: String, node: Option<&SyntaxNode>, file, range, test| {
        let Some((which, first, value)) = declared_type(cx, ty) else {
            return;
        };
        let (name_label, field_label) = match which {
            Which::Collection => ("name", "key"),
            Which::Index => ("collection", "field"),
        };
        out.push(Declared {
            which,
            declaration,
            name: node.and_then(|n| literal_arg(n, name_label)),
            field: node.and_then(|n| literal_arg(n, field_label)),
            first,
            value,
            file,
            range,
            test,
            breaking: node.is_some_and(has_breaking),
        });
    };
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let file = p.file(decl.file);
        if file.stdlib {
            continue;
        }
        let test = file.is_test;
        if decl.named_kind() == Some(K::Constant) {
            let ty = cx.constant_ty(d);
            push(
                &ty,
                p.decl_name(d),
                Some(&decl.node),
                decl.file,
                decl.name_range,
                test,
            );
            continue;
        }
        for &m in &decl.members {
            let member = p.member(m);
            if member.kind != MemberKind::Field {
                continue;
            }
            let ty = cx.field_ty(m);
            push(
                &ty,
                format!("{}.{}", p.decl_name(d), member.name),
                member.node.as_ref(),
                decl.file,
                member.name_range,
                test,
            );
        }
    }
    out
}

/// [`find`], dropping the diagnostics of the types it resolves.
pub(crate) fn declarations(cx: &Cx<'_, '_>) -> Vec<Declared> {
    let reported = cx.diags.borrow().len();
    let out = find(cx);
    cx.diags.borrow_mut().truncate(reported);
    out
}

/// The key kind of `ty`, `None` for a type that can't be a key.
fn key_kind(cx: &Cx<'_, '_>, ty: &Ty) -> Option<KeyKind> {
    let Ty::Named(d, args) = ty else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let d = *d;
    if Some(d) == cx.b.string || Some(d) == cx.p.lookup_qualified("k.crypto.Uuid") {
        return Some(KeyKind::Text);
    }
    if Some(d) == cx.b.int {
        return Some(KeyKind::Int);
    }
    if Some(d) == cx.p.lookup_qualified("k.core.Date") {
        return Some(KeyKind::Date);
    }
    if cx.p.decl(d).named_kind() == Some(K::Enum)
        && !cx.cases(d).iter().any(|&c| cx.case_has_payload(c))
    {
        return Some(KeyKind::Text);
    }
    None
}

/// The `@JsonKey("…")` of member `m`, else its name.
fn json_key(cx: &Cx<'_, '_>, m: MemberId) -> String {
    let member = cx.p.member(m);
    member
        .node
        .as_ref()
        .and_then(|n| {
            n.children()
                .filter_map(ast::Annotation::cast)
                .find(|a| a.name().is_some_and(|t| t.text() == "JsonKey"))
        })
        .and_then(|a| a.arg_list()?.args().next()?.value())
        .and_then(|v| match v {
            ast::Expr::StringLit(s) => s.plain_value(),
            _ => None,
        })
        .unwrap_or_else(|| member.name.clone())
}

/// The stored field `name` of struct type `value`: the field, its type
/// and its JSON key; `Err` with why not.
fn stored_field(cx: &Cx<'_, '_>, value: &Ty, name: &str) -> Result<(Ty, String), String> {
    let p = cx.p;
    let shown = value.render(p);
    let Ty::Named(d, _) = value else {
        return Err(format!(
            "`{shown}` isn't a struct, so it has no fields to key or index by"
        ));
    };
    if p.decl(*d).named_kind() != Some(K::Struct) {
        return Err(format!(
            "`{shown}` isn't a struct, so it has no fields to key or index by"
        ));
    }
    let field = cx
        .stored_fields(*d)
        .into_iter()
        .find(|&m| p.member(m).name == name);
    match field {
        Some(m) => Ok((cx.field_ty(m), json_key(cx, m))),
        None => Err(format!("`{shown}` has no stored field named `{name}`")),
    }
}

/// The stored fields of struct type `value` whose type, without one `?`,
/// is a key type.
fn keyable_fields(cx: &Cx<'_, '_>, value: &Ty) -> Vec<CollectionField> {
    let Ty::Named(d, _) = value else {
        return Vec::new();
    };
    cx.stored_fields(*d)
        .into_iter()
        .filter_map(|m| {
            let ty = cx.field_ty(m);
            let inner = match &ty {
                Ty::Optional(inner) => (**inner).clone(),
                other => other.clone(),
            };
            Some(CollectionField {
                field: cx.p.member(m).name.clone(),
                json: json_key(cx, m),
                kind: key_kind(cx, &inner)?,
            })
        })
        .collect()
}

/// The key types, for messages.
const KEY_TYPES: &str = "`String`, `Int`, `Uuid`, `Date` or an enum without payloads";

/// The field a [`Declared`] names, checked (K2411): `None` when it isn't
/// valid or its types don't resolve, after reporting why.
fn checked_field(cx: &Cx<'_, '_>, d: &Declared, report: bool) -> Option<CollectionField> {
    if d.first.contains_error() || d.value.contains_error() {
        return None;
    }
    let p = cx.p;
    let (label, what, a_what) = match d.which {
        Which::Collection => ("key", "key", "a key"),
        Which::Index => ("field", "indexed field", "an indexed field"),
    };
    let fail = |range: TextRange, message: String| {
        if report {
            cx.report(
                d.file,
                Diagnostic::new(
                    codes::INVALID_COLLECTION_FIELD,
                    p.span(d.file, range),
                    message,
                ),
            );
        }
        None
    };
    let (name, range) = match &d.field {
        Some(Ok(lit)) => lit.clone(),
        Some(Err(range)) => {
            return fail(
                *range,
                format!(
                    "`{label}:` takes the name of a field as a string literal: the {what} is part of the stored format"
                ),
            );
        }
        None => return None,
    };
    let (ty, json) = match stored_field(cx, &d.value, &name) {
        Ok(f) => f,
        Err(why) => return fail(range, why),
    };
    let (inner, optional) = match &ty {
        Ty::Optional(inner) => ((**inner).clone(), true),
        other => (other.clone(), false),
    };
    if ty.contains_error() {
        return None;
    }
    if optional && d.which == Which::Collection {
        return fail(
            range,
            format!(
                "the key field `{name}` is `{}`: every value needs a key, so the key field can't be optional",
                ty.render(p)
            ),
        );
    }
    if inner != d.first {
        return fail(
            range,
            format!(
                "the field `{name}` is `{}`, but the {what} type is `{}`",
                ty.render(p),
                d.first.render(p)
            ),
        );
    }
    let Some(kind) = key_kind(cx, &inner) else {
        return fail(
            range,
            format!(
                "the field `{name}` is `{}`, which IndexedDB can't use as a key; {a_what} is {KEY_TYPES}",
                ty.render(p)
            ),
        );
    };
    Some(CollectionField {
        field: name,
        json,
        kind,
    })
}

/// K2411 for a `Collection(…)` or `CollectionIndex(…)` written anywhere
/// but the initializer of a constant or a field of non-test code (a local
/// `let`, an argument): the compiler finds collections by their
/// declarations, so one made inline would be missing from the browser's
/// database and from `storage.lock`. Calls are found by their callee's
/// name in files importing the stdlib type.
fn inline_declarations(cx: &Cx<'_, '_>) {
    use k_syntax::SyntaxKind as S;
    let p = cx.p;
    for (ix, file) in p.files.iter().enumerate() {
        if file.stdlib || file.is_test {
            continue;
        }
        let imports = |name: &str| {
            file.root
                .children()
                .filter(|n| n.kind() == S::Import)
                .any(|n| n.text().to_string().split_whitespace().any(|w| w == name))
        };
        let names: Vec<&str> = [
            ("Collection", "k.storage.Collection"),
            ("CollectionIndex", "k.storage.CollectionIndex"),
        ]
        .into_iter()
        .filter(|(_, q)| imports(q))
        .map(|(n, _)| n)
        .collect();
        if names.is_empty() {
            continue;
        }
        for call in file.root.descendants().filter_map(ast::CallExpr::cast) {
            let callee = call
                .syntax()
                .first_token()
                .map(|t| t.text().to_owned())
                .unwrap_or_default();
            if !names.contains(&callee.as_str()) {
                continue;
            }
            let declared = call
                .syntax()
                .ancestors()
                .skip(1)
                .find(|a| {
                    matches!(
                        a.kind(),
                        S::FieldDecl | S::LetStmt | S::FuncDecl | S::InitDecl | S::ArgList
                    )
                })
                .is_some_and(|a| a.kind() == S::FieldDecl);
            if declared {
                continue;
            }
            let ix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
            cx.report(
                ix,
                Diagnostic::new(
                    codes::INVALID_COLLECTION_FIELD,
                    p.span(ix, call.syntax().text_range()),
                    format!(
                        "a `{callee}` is declared as a constant or a field, where the compiler finds it to set up the browser's database and record it in storage.lock"
                    ),
                ),
            );
        }
    }
}

/// K2403 for a `name:` or `collection:` that isn't a plain string literal:
/// it is part of the stored format, as a storage key's name is.
fn literal_names(cx: &Cx<'_, '_>, all: &[Declared]) {
    for d in all {
        if let Some(Err(range)) = d.name {
            let label = match d.which {
                Which::Collection => "Collection(name:)",
                Which::Index => "CollectionIndex(collection:)",
            };
            cx.report(
                d.file,
                Diagnostic::new(
                    codes::LITERAL_REQUIRED,
                    cx.p.span(d.file, range),
                    format!(
                        "`{label}` takes a string literal: the name is part of the stored format, recorded in storage.lock"
                    ),
                ),
            );
        }
    }
}

/// Runs the checks (K2403, K2410, K2411, K2412) and returns the schema: every
/// collection with a valid name and key, with its valid indexes, sorted by
/// name; a test build's collections included (marked `test`).
pub(crate) fn check(cx: &Cx<'_, '_>) -> Vec<CollectionSchema> {
    inline_declarations(cx);
    let all = declarations(cx);
    literal_names(cx, &all);
    let p = cx.p;
    // Collections by name: (file, literal range, value type, test).
    let mut collections: BTreeMap<String, (FileIx, TextRange, Ty, bool, Option<CollectionField>)> =
        BTreeMap::new();
    for d in all.iter().filter(|d| d.which == Which::Collection) {
        let field = checked_field(cx, d, !d.test);
        let Some(Ok((name, literal))) = &d.name else {
            continue;
        };
        match collections.get(name) {
            Some((file, range, _, false, _)) if !d.test => cx.report(
                d.file,
                Diagnostic::new(
                    codes::DUPLICATE_COLLECTION,
                    p.span(d.file, *literal),
                    format!(
                        "another collection is named `{name}`; the two would share one IndexedDB store"
                    ),
                )
                .with_label(
                    p.span(*file, *range),
                    format!("the collection named `{name}` is declared here"),
                ),
            ),
            Some((_, _, _, true, _)) if !d.test => {
                // A non-test declaration wins over a test's.
                collections.insert(
                    name.clone(),
                    (d.file, *literal, d.value.clone(), false, field),
                );
            }
            Some(_) => {}
            None => {
                collections.insert(
                    name.clone(),
                    (d.file, *literal, d.value.clone(), d.test, field),
                );
            }
        }
    }
    let mut indexes: BTreeMap<(String, String), (FileIx, TextRange)> = BTreeMap::new();
    let mut by_collection: BTreeMap<String, BTreeMap<String, CollectionField>> = BTreeMap::new();
    // The indexes only test code declares, kept apart: they join a test
    // build's database, never the app's or `storage.lock`.
    let mut by_collection_in_tests: BTreeMap<String, BTreeMap<String, CollectionField>> =
        BTreeMap::new();
    for d in all.iter().filter(|d| d.which == Which::Index) {
        let Some(Ok((collection, literal))) = &d.name else {
            continue;
        };
        if d.value.contains_error() || d.first.contains_error() {
            continue;
        }
        let known = collections.get(collection);
        match known {
            Some((_, _, value, _, _)) if *value == d.value => {}
            Some((file, range, value, _, _)) => {
                if !d.test {
                    cx.report(
                        d.file,
                        Diagnostic::new(
                            codes::UNKNOWN_COLLECTION,
                            p.span(d.file, *literal),
                            format!(
                                "the collection `{collection}` holds `{}`, not `{}`",
                                value.render(p),
                                d.value.render(p)
                            ),
                        )
                        .with_label(
                            p.span(*file, *range),
                            format!("the collection `{collection}` is declared here"),
                        ),
                    );
                }
                continue;
            }
            None => {
                if !d.test {
                    cx.report(
                        d.file,
                        Diagnostic::new(
                            codes::UNKNOWN_COLLECTION,
                            p.span(d.file, *literal),
                            format!("no `Collection` is named `{collection}`"),
                        ),
                    );
                }
                continue;
            }
        }
        let Some(field) = checked_field(cx, d, !d.test) else {
            continue;
        };
        if !d.test {
            let at = (collection.clone(), field.field.clone());
            if let Some(&(file, range)) = indexes.get(&at) {
                cx.report(
                    d.file,
                    Diagnostic::new(
                        codes::DUPLICATE_COLLECTION,
                        p.span(d.file, *literal),
                        format!(
                            "another index of `{collection}` is on the field `{}`; an index's name is its field, so there is one per field",
                            field.field
                        ),
                    )
                    .with_label(p.span(file, range), "that index is declared here"),
                );
                continue;
            }
            indexes.insert(at, (d.file, *literal));
        }
        let into = if d.test {
            &mut by_collection_in_tests
        } else {
            &mut by_collection
        };
        into.entry(collection.clone())
            .or_default()
            .insert(field.field.clone(), field);
    }
    collections
        .into_iter()
        .filter_map(|(name, (_, _, value, test, key))| {
            let key = key?;
            let indexes = by_collection.remove(&name).unwrap_or_default();
            let test_indexes = by_collection_in_tests
                .remove(&name)
                .unwrap_or_default()
                .into_iter()
                .filter(|(f, _)| !indexes.contains_key(f))
                .map(|(_, i)| i)
                .collect();
            Some(CollectionSchema {
                name,
                key,
                indexes: indexes.into_values().collect(),
                test_indexes,
                fields: keyable_fields(cx, &value),
                test,
            })
        })
        .collect()
}

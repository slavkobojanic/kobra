//! Element references (`docs/design/53-element-references.md`): the
//! program's `ElementRef` fields and the `.ref(…)` attributes that attach
//! them, checked after every body recorded its resolutions.
//!
//! * **K3113**: an `ElementRef` field that isn't a plain `let` of a
//!   component, page or layout initialized with `ElementRef()`; an
//!   `ElementRef()` anywhere else; a `.ref(…)` whose argument isn't such a
//!   field of the declaration whose view it is;
//! * **K3114**: a second `.ref(…)` of the same reference (label at the
//!   first), or one inside a `for` body or a scoped slot (a slot lambda
//!   with parameters), which may render the element any number of times;
//! * **K3115** (warning): a reference no `.ref(…)` attaches.
//!
//! K3116 (an operation called while a view renders) is the body check's,
//! at the call. Test files aren't checked: a test may build a reference to
//! pass to a helper.

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, FileId, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Cx, Found};
use crate::lookup::Resolution;
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// `true` when `e` is `ElementRef()`: the name, called with no arguments.
fn is_construction(e: &ast::Expr) -> bool {
    let ast::Expr::CallExpr(c) = e else {
        return false;
    };
    let named =
        matches!(c.callee(), Some(ast::Expr::NameExpr(n)) if n.syntax().text() == "ElementRef");
    named && c.trailing_lambda().is_none() && c.arg_list().is_none_or(|l| l.args().next().is_none())
}

/// The `.ref(…)` call whose link name is at `range` in `root`, and its
/// argument.
fn ref_call(root: &SyntaxNode, range: TextRange) -> Option<(ast::CallExpr, Option<ast::Expr>)> {
    let token = root.covering_element(range).into_token()?;
    let call = token
        .parent_ancestors()
        .filter_map(ast::CallExpr::cast)
        .find(|c| {
            c.callee()
                .is_some_and(|e| e.syntax().text_range().contains_range(range))
        })?;
    let value = call.arg_list()?.args().next().and_then(|a| a.value());
    Some((call, value))
}

/// `true` when the node at `call` sits in a `for` body or a lambda with
/// parameters (a scoped slot) below `decl`.
fn repeated(call: &SyntaxNode, decl: &SyntaxNode) -> bool {
    for n in call.ancestors() {
        if &n == decl {
            break;
        }
        match n.kind() {
            SyntaxKind::ForStmt => return true,
            SyntaxKind::LambdaExpr => {
                let params = ast::LambdaExpr::cast(n.clone())
                    .and_then(|l| l.param_list())
                    .is_some_and(|p| p.syntax().children().next().is_some());
                if params {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Runs the checks.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let Some(element_ref) = p.lookup_qualified("k.ui.ElementRef") else {
        return;
    };
    // The `.ref(…)` attributes by file, and the member resolutions their
    // arguments may need, copied out of the borrows.
    let mut attrs: HashMap<FileId, Vec<TextRange>> = HashMap::new();
    let mut members: HashMap<(FileId, TextRange), Resolution> = HashMap::new();
    {
        let live = cx.resolutions.borrow();
        let chunks = cx.body_resolutions.borrow();
        for ((f, r), res) in live.iter().chain(chunks.iter().flat_map(|c| c.iter())) {
            match res {
                Resolution::Attribute { name, .. } if name == "ref" => {
                    attrs.entry(*f).or_default().push(*r);
                }
                Resolution::Member { .. } => {
                    members.entry((*f, *r)).or_insert_with(|| res.clone());
                }
                _ => {}
            }
        }
    }
    for list in attrs.values_mut() {
        list.sort_by_key(|r| (r.start(), r.end()));
        list.dedup();
    }
    let is_ref = |ty: &Ty| matches!(ty, Ty::Named(d, args) if *d == element_ref && args.is_empty());
    for (ix, file) in p.files.iter().enumerate() {
        if file.stdlib || file.is_test {
            continue;
        }
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        // Initializers of reference fields, valid or reported at the field.
        let mut field_inits: Vec<TextRange> = Vec::new();
        for &d in &file.decls {
            check_decl(
                cx,
                fix,
                d,
                &is_ref,
                attrs.get(&file.id),
                &members,
                &mut field_inits,
            );
        }
        // `ElementRef()` anywhere else.
        if !matches!(cx.lookup(fix, "ElementRef"), Found::Decl(x) if x == element_ref) {
            continue;
        }
        for c in file.root.descendants().filter_map(ast::CallExpr::cast) {
            let e = ast::Expr::CallExpr(c.clone());
            let range = c.syntax().text_range();
            if is_construction(&e) && !field_inits.contains(&range) {
                cx.report(
                    fix,
                    Diagnostic::new(
                        codes::MISPLACED_ELEMENT_REF,
                        p.span(fix, range),
                        "an element reference is made only as a field of the component that names its element",
                    )
                    .with_help("declare `let name = ElementRef()` in the component, page or layout, and attach it with `.ref(name)`"),
                );
            }
        }
    }
}

/// The checks of one declaration's reference fields and `.ref(…)`
/// attributes.
fn check_decl(
    cx: &Cx<'_, '_>,
    file: FileIx,
    d: DeclId,
    is_ref: &dyn Fn(&Ty) -> bool,
    attrs: Option<&Vec<TextRange>>,
    members: &HashMap<(FileId, TextRange), Resolution>,
    field_inits: &mut Vec<TextRange>,
) {
    let p = cx.p;
    let decl = p.decl(d);
    let viewed = matches!(decl.named_kind(), Some(K::Component | K::Page | K::Layout));
    // The valid reference fields: by resolution, with their uses.
    let mut fields: Vec<(MemberId, Resolution, Vec<TextRange>)> = Vec::new();
    for &m in &decl.members {
        let member = p.member(m);
        if member.kind != MemberKind::Field || !is_ref(&cx.field_ty(m)) {
            continue;
        }
        let init = member
            .node
            .clone()
            .and_then(ast::FieldDecl::cast)
            .and_then(|f| f.initializer());
        if let Some(i) = &init {
            field_inits.push(i.syntax().text_range());
        }
        let constructed = init.as_ref().is_some_and(is_construction);
        let why = if !viewed {
            Some(format!(
                "`{}` isn't a component, page or layout, so it has no view whose element a reference could name",
                decl.name
            ))
        } else if member.is_var || member.is_static || !member.annotations.is_empty() {
            Some("an element reference is a plain `let` field: it names one element for the component's lifetime".to_owned())
        } else if !constructed {
            Some("an element reference field is initialized with `ElementRef()`".to_owned())
        } else {
            None
        };
        match why {
            Some(message) => cx.report(
                file,
                Diagnostic::new(
                    codes::MISPLACED_ELEMENT_REF,
                    p.span(file, member.name_range),
                    message,
                )
                .with_help(format!(
                    "declare `let {} = ElementRef()` in the component whose view has the element",
                    member.name
                )),
            ),
            None => fields.push((m, cx.member_resolution(m), Vec::new())),
        }
    }
    let id = p.file(file).id;
    let node = decl.node.clone();
    let range = node.text_range();
    let root = &p.file(file).root;
    for &r in attrs
        .into_iter()
        .flatten()
        .filter(|r| range.contains_range(**r))
    {
        let Some((call, value)) = ref_call(root, r) else {
            continue;
        };
        let Some(value) = value else { continue };
        let at = value.syntax().text_range();
        let resolved = match &value {
            ast::Expr::NameExpr(n) => n.syntax().first_token().and_then(|t| {
                members
                    .get(&(id, t.text_range()))
                    .or_else(|| members.get(&(id, at)))
            }),
            _ => None,
        };
        let Some(field) = resolved.and_then(|res| fields.iter_mut().find(|(_, x, _)| x == res))
        else {
            // A field reported above already has its error.
            let reported = resolved.is_some_and(|res| {
                decl.members.iter().any(|&m| {
                    p.member(m).kind == MemberKind::Field
                        && is_ref(&cx.field_ty(m))
                        && &cx.member_resolution(m) == res
                })
            });
            if !reported {
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::MISPLACED_ELEMENT_REF,
                        p.span(file, at),
                        "`.ref(…)` takes an element reference field of this component",
                    )
                    .with_help(
                        "declare `let name = ElementRef()` in this component and pass `name`",
                    ),
                );
            }
            continue;
        };
        if repeated(call.syntax(), &node) {
            cx.report(
                file,
                Diagnostic::new(
                    codes::ELEMENT_REF_REUSED,
                    p.span(file, at),
                    "this element may render any number of times, but a reference names one element",
                )
                .with_help("move the element into a component of its own, which holds its own reference"),
            );
        } else if let Some(&first) = field.2.first() {
            cx.report(
                file,
                Diagnostic::new(
                    codes::ELEMENT_REF_REUSED,
                    p.span(file, at),
                    format!("`{}` already names an element", p.member(field.0).name),
                )
                .with_label(p.span(file, first), "attached here")
                .with_help("give each element a reference of its own"),
            );
        }
        field.2.push(at);
    }
    for (m, _, uses) in &fields {
        if uses.is_empty() {
            let member = p.member(*m);
            cx.report(
                file,
                Diagnostic::new(
                    codes::UNATTACHED_ELEMENT_REF,
                    p.span(file, member.name_range),
                    format!(
                        "no element is attached to `{}`, so its operations do nothing",
                        member.name
                    ),
                )
                .with_help(format!(
                    "attach it to an element of the view with `.ref({})`",
                    member.name
                )),
            );
        }
    }
}

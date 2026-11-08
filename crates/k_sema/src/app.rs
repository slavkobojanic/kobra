//! Configs and test groups (sema.md §12.11, §12.6 "`use` operands",
//! §12.14).
//!
//! * A `config` holds only `@Env` fields (K4017) of `ConfigValue` types
//!   (K4019); two fields never name the same variable (K4018).
//! * A test group's `use` rules: expressions bind configs only (K4011), a
//!   type operand is or implements its target (K4010), restating a test
//!   default is a warning (K4014), and a group whose dependencies reach an
//!   app config binds it (K7503).
//! * `implements` a service is for fakes in test code (K4021); a fake
//!   provides every non-private member of the service (K2010).

use std::collections::{HashMap, HashSet};

use k_diagnostics::{Applicability, Diagnostic, Fix, TextEdit, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::body::{BodyOwner, Checker};
use crate::cx::Cx;
use crate::program::{DeclId, FileIx, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// The config and test-group checks over the whole program.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let mut envs: HashMap<String, (FileIx, TextRange)> = HashMap::new();
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        match decl.named_kind() {
            Some(K::Config) => config(cx, d, &mut envs),
            Some(K::TestSuite) if !decl.name.is_empty() => test_group(cx, d),
            _ => {}
        }
        if decl.named_kind() == Some(K::Service) {
            fakes(cx, d);
        }
    }
}

/// The rules of one `config` declaration (K4017, K4019) and its `@Env` names, recorded in `envs` to find duplicates across configs (K4018).
fn config(cx: &Cx<'_, '_>, d: DeclId, envs: &mut HashMap<String, (FileIx, TextRange)>) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = decl.file;
    let config_value = p.lookup_qualified("k.core.ConfigValue");
    for &m in &decl.members {
        let member = p.member(m);
        match member.kind {
            MemberKind::Func => cx.error(
                file,
                codes::CONFIG_MEMBER_NOT_ALLOWED,
                member.name_range,
                "a config is data read from the environment; it has no functions",
            ),
            MemberKind::Field | MemberKind::Computed => {
                let Some(node) = &member.node else { continue };
                let env = node
                    .children()
                    .filter_map(ast::Annotation::cast)
                    .find(|a| a.name().is_some_and(|n| n.text() == "Env"));
                let Some(env) = env else {
                    // A field whose annotation doesn't apply has K3002.
                    if node.children().any(|c| c.kind() == SyntaxKind::Annotation) {
                        continue;
                    }
                    cx.error(
                        file,
                        codes::CONFIG_MEMBER_NOT_ALLOWED,
                        member.name_range,
                        "every config field names its variable with `@Env(\"NAME\")`",
                    );
                    continue;
                };
                let ty = cx.field_ty(m);
                let inner = match &ty {
                    Ty::Optional(i) => (**i).clone(),
                    t => t.clone(),
                };
                if !inner.contains_error() && !is_config_value(cx, &inner, config_value) {
                    let range = ast::FieldDecl::cast(node.clone())
                        .and_then(|f| f.ty())
                        .map_or(member.name_range, |t| t.syntax().text_range());
                    cx.error(
                        file,
                        codes::NOT_A_CONFIG_VALUE,
                        range,
                        format!(
                            "a config field is a `ConfigValue`; `{}` isn't",
                            ty.render(p)
                        ),
                    );
                }
                let Some(lit) = env
                    .arg_list()
                    .and_then(|l| l.args().next())
                    .and_then(|a| a.value())
                    .filter(|v| matches!(v, ast::Expr::StringLit(_)))
                else {
                    continue;
                };
                let name: String = lit
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind() == SyntaxKind::StringText)
                    .map(|t| t.text().to_owned())
                    .collect();
                let range = lit.syntax().text_range();
                match envs.get(&name) {
                    Some((first_file, first)) => cx.report(
                        file,
                        Diagnostic::new(
                            codes::DUPLICATE_ENV_VARIABLE,
                            p.span(file, range),
                            format!("`{name}` is read by another config field"),
                        )
                        .with_label(p.span(*first_file, *first), "first read here"),
                    ),
                    None => {
                        envs.insert(name, (file, range));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Returns `true` for `ConfigValue` types, enums without payloads (read
/// by case name) and lists of either (comma-separated).
fn is_config_value(cx: &Cx<'_, '_>, ty: &Ty, config_value: Option<DeclId>) -> bool {
    match ty {
        Ty::Error => true,
        Ty::Named(d, args) if Some(*d) == cx.b.list => args
            .first()
            .is_some_and(|a| is_config_value(cx, a, config_value)),
        Ty::Named(d, _) if cx.is_enum(*d) && !cx.p.is_stdlib(*d) => {
            cx.cases(*d).iter().all(|c| !cx.case_has_payload(*c))
        }
        _ => config_value.is_some_and(|c| cx.implements(ty, c)),
    }
}

/// The `use` rules directly in a test group.
fn use_decls(node: &SyntaxNode) -> Vec<ast::UseDecl> {
    node.children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
        .map(|l| l.children().filter_map(ast::UseDecl::cast).collect())
        .unwrap_or_default()
}

/// The `use` rules of a test group and the configs its dependencies reach (K4011, K4010, K4014, K7503).
fn test_group(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = decl.file;
    let owner = BodyOwner {
        file,
        decl: None,
        self_ty: None,
        generics: Vec::new(),
    };
    let mut checker = Checker::new(cx, owner);
    let mut bound: HashSet<DeclId> = HashSet::new();
    let mut replaced: HashMap<DeclId, DeclId> = HashMap::new();
    for u in use_decls(&decl.node) {
        let Some(target_node) = u.target() else {
            continue;
        };
        let target = cx.resolve_type(file, &[], &target_node);
        let Some(t) = target.decl() else { continue };
        bound.insert(t);
        if u.implementation().is_none() {
            if let Some(e) = u.value() {
                if p.decl(t).named_kind() != Some(K::Config) {
                    checker.silently(|c| {
                        c.check_initializer(&e, None);
                    });
                    cx.error(
                        file,
                        codes::USE_EXPRESSION_NOT_ALLOWED,
                        e.syntax().text_range(),
                        "`use` binds a value only to a `config`; other targets take a type",
                    );
                } else {
                    checker.check_initializer(&e, Some(&target));
                }
            }
            continue;
        }
        let Some(imp_node) = u.implementation() else {
            continue;
        };
        let imp = cx.resolve_type(file, &[], &imp_node);
        let Some(i) = imp.decl() else { continue };
        replaced.insert(t, i);
        if i != t && !cx.declares(i, t) {
            cx.error(
                file,
                codes::USE_TARGET_MISMATCH,
                imp_node.syntax().text_range(),
                format!(
                    "`{}` doesn't implement `{}`",
                    p.decl_name(i),
                    p.decl_name(t)
                ),
            );
            continue;
        }
        if test_default(cx, i) == Some(t) {
            let range = TextRange::new(
                u.syntax().text_range().start(),
                target_node.syntax().text_range().end(),
            );
            let line = cx.line_range(file, u.syntax().text_range());
            cx.report(
                file,
                Diagnostic::new(
                    codes::USE_RESTATES_DEFAULT,
                    p.span(file, range),
                    format!(
                        "`{}` is already the test default for `{}`",
                        p.decl_name(i),
                        p.decl_name(t)
                    ),
                )
                .with_fix(remove_rule_fix(cx, file, u.syntax(), line)),
            );
        }
    }
    checker.finish();
    // K7503: the group's dependencies reach an app config it doesn't bind.
    // A depth-first walk with a parent map; the path is spelled only for
    // the report (spelling it at every step made the walk quadratic in the
    // path length, 13% of a warm re-check at 100k lines). Without `use`
    // replacements, a declaration from which no config is reachable
    // (`di::reaches_config`) can't lead to a report, nor can anything
    // reachable from it, so it isn't visited: the walk finds the same
    // config along the same path, and a group over a long service chain
    // no longer walks the whole chain.
    let prune = replaced.is_empty();
    // Reached declaration (after replacement) → the declaration it was
    // reached from and the injected type as written (before replacement),
    // which is what the path names.
    let mut parent: HashMap<DeclId, (Option<DeclId>, DeclId)> = HashMap::new();
    let mut stack: Vec<(DeclId, Option<DeclId>)> = crate::di::injected_decls(cx, d)
        .iter()
        .filter(|t| !prune || crate::di::reaches_config(cx, **t))
        .map(|t| (*t, None))
        .collect();
    while let Some((written, from)) = stack.pop() {
        let t = replaced.get(&written).copied().unwrap_or(written);
        if parent.contains_key(&t) {
            continue;
        }
        parent.insert(t, (from, written));
        match p.decl(t).named_kind() {
            Some(K::Config) if !p.is_stdlib(t) && !bound.contains(&t) => {
                let mut names = vec![p.decl(written).name.as_str()];
                let mut at = from;
                while let Some((up, name)) = at.and_then(|x| parent.get(&x)) {
                    names.push(p.decl(*name).name.as_str());
                    at = *up;
                }
                names.push(decl.name.as_str());
                names.reverse();
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::UNBOUND_TEST_CONFIG,
                        p.span(file, decl.name_range),
                        format!(
                            "tests never read the environment: bind `{}` with `use … for {}`",
                            p.decl(t).name,
                            p.decl(t).name
                        ),
                    )
                    .with_note(names.join(" → ")),
                );
                return;
            }
            Some(K::Service | K::Repository) => {
                stack.extend(
                    crate::di::injected_decls(cx, t)
                        .iter()
                        .filter(|x| !prune || crate::di::reaches_config(cx, **x))
                        .map(|x| (*x, Some(t))),
                );
            }
            _ => {}
        }
    }
}

/// The declaration a stdlib test default replaces (`@TestDefault(replaces:
/// Clock)` on `TestClock`).
fn test_default(cx: &Cx<'_, '_>, d: DeclId) -> Option<DeclId> {
    let decl = cx.p.decl(d);
    if !cx.p.is_stdlib(d) {
        return None;
    }
    let a = decl
        .node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|n| n.text() == "TestDefault"))?;
    let name = a
        .arg_list()?
        .args()
        .next()?
        .value()?
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident)?;
    match cx.lookup(decl.file, name.text()) {
        crate::cx::Found::Decl(x) => Some(x),
        _ => None,
    }
}

/// `service Fake implements CartService`: only in test code (K4021), and
/// complete (K2010).
fn fakes(cx: &Cx<'_, '_>, d: DeclId) {
    let p = cx.p;
    let decl = p.decl(d);
    let file = p.file(decl.file);
    let test_code = file.is_test
        || file
            .module
            .is_some_and(|m| p.modules[m.0 as usize].test_only);
    for (ty, range) in cx.implements_list(d).iter() {
        let Some(s) = ty.decl() else { continue };
        if p.decl(s).named_kind() != Some(K::Service) {
            continue;
        }
        if !test_code {
            cx.error(
                decl.file,
                codes::IMPLEMENTS_SERVICE,
                *range,
                "only fakes in test code implement a service; production code depends on interfaces",
            );
            continue;
        }
        let missing: Vec<String> = p
            .decl(s)
            .members
            .iter()
            .map(|m| p.member(*m))
            .filter(|m| {
                matches!(
                    m.kind,
                    MemberKind::Func | MemberKind::Field | MemberKind::Computed
                ) && m.visibility != ast::Visibility::Private
                    && !m.has_annotation("Inject")
                    && !decl.member_names.contains_key(&m.name)
            })
            .map(|m| format!("`{}`", m.name))
            .collect();
        if !missing.is_empty() {
            cx.error(
                decl.file,
                codes::MISSING_INTERFACE_MEMBER,
                *range,
                format!("`{}` is missing {}", decl.name, missing.join(", ")),
            );
        }
    }
}

/// The K4014 fix: removes the `use` rule's line (`line`) and the imports
/// that only the rule used ([`Cx::imports_only_in`]), so applying it leaves
/// no unused import (K1105). Edits are in source order: the imports come
/// before the rule.
fn remove_rule_fix(cx: &Cx<'_, '_>, file: FileIx, rule: &SyntaxNode, line: TextRange) -> Fix {
    let p = cx.p;
    let mut edits = cx.imports_only_in(file, rule.text_range(), &HashSet::new());
    edits.push(TextEdit {
        span: p.span(file, line),
        replacement: String::new(),
    });
    edits.sort_by_key(|e| e.span.range.start());
    Fix {
        message: "Remove the `use` rule".to_owned(),
        edits,
        applicability: Applicability::MachineApplicable,
    }
}

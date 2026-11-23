//! Rendering of types, signatures and declarations as K source, for hover,
//! completion details and signature help.

use std::fmt::Write as _;

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{
    DeclId, DeclKind, GenId, MemberId, MemberKind, Param, ProgramView, Signature, Type,
};

/// Renders `ty` as K type syntax (`List<shop.Item>`, `Int?`,
/// `(Int) async throws E -> Bool`). Prelude declarations are written by
/// their simple name, others fully qualified, as diagnostics write them.
#[must_use]
pub fn ty(view: &ProgramView, ty: &Type) -> String {
    ty_with(view, ty, &[])
}

/// Renders `ty` like [`ty`], each generic parameter in `subst` written as
/// its type argument there (`T` as `User` in `decode<User>(json)`).
#[must_use]
pub fn ty_with(view: &ProgramView, ty: &Type, subst: &[(GenId, String)]) -> String {
    let mut out = String::new();
    write_ty(view, ty, subst, &mut out);
    out
}

/// Appends `ty` to `out` as [`ty_with`] renders it; unknown declarations
/// and error types are written `{error}`.
fn write_ty(view: &ProgramView, ty: &Type, subst: &[(GenId, String)], out: &mut String) {
    match ty {
        Type::Error => out.push_str("{error}"),
        Type::Never => out.push_str("Never"),
        Type::Named(d, args) => {
            match view.decls.get(d.0 as usize) {
                Some(decl) => out.push_str(&decl.display_name),
                None => out.push_str("{error}"),
            }
            if !args.is_empty() {
                out.push('<');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_ty(view, a, subst, out);
                }
                out.push('>');
            }
        }
        Type::Optional(inner) => {
            let needs_parens = matches!(**inner, Type::Func(_));
            if needs_parens {
                out.push('(');
            }
            write_ty(view, inner, subst, out);
            if needs_parens {
                out.push(')');
            }
            out.push('?');
        }
        Type::Func(f) => {
            out.push('(');
            for (i, p) in f.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_ty(view, p, subst, out);
            }
            out.push(')');
            if f.is_async {
                out.push_str(" async");
            }
            if let Some(e) = &f.throws {
                out.push_str(" throws ");
                write_ty(view, e, subst, out);
            }
            out.push_str(" -> ");
            write_ty(view, &f.ret, subst, out);
        }
        Type::Tuple(items) => {
            out.push('(');
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_ty(view, t, subst, out);
            }
            out.push(')');
        }
        Type::Param(g) if subst.iter().any(|(s, _)| s == g) => {
            if let Some((_, text)) = subst.iter().find(|(s, _)| s == g) {
                out.push_str(text);
            }
        }
        Type::Param(g) => match view.generics.get(g.0 as usize) {
            Some(info) => out.push_str(&info.name),
            None => out.push_str("{error}"),
        },
        Type::Meta(inner) => write_ty(view, inner, subst, out),
    }
}

/// Renders one parameter as declared: `label name: Type`, `_ name: Type`
/// or `name: Type`, with ` = …` for a default.
#[must_use]
pub fn param(view: &ProgramView, p: &Param) -> String {
    param_with(view, p, &[])
}

/// Renders one parameter like [`param`], with the generic parameters in
/// `subst` written as their type arguments ([`ty_with`]).
#[must_use]
pub fn param_with(view: &ProgramView, p: &Param, subst: &[(GenId, String)]) -> String {
    let mut out = String::new();
    match &p.label {
        Some(label) if *label != p.name => {
            let _ = write!(out, "{label} ");
        }
        Some(_) => {}
        None => out.push_str("_ "),
    }
    let _ = write!(out, "{}: {}", p.name, ty_with(view, &p.ty, subst));
    if p.has_default {
        out.push_str(" = …");
    }
    out
}

/// Renders the part of a signature after the name: generics, parameters,
/// effects and the result (`<T>(item: T) async throws E -> Bool`). A `Void`
/// result is left out.
#[must_use]
pub fn signature_tail(view: &ProgramView, sig: &Signature) -> String {
    let mut out = String::new();
    if !sig.generics.is_empty() {
        out.push('<');
        for (i, g) in sig.generics.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            if let Some(info) = view.generics.get(g.0 as usize) {
                out.push_str(&info.name);
            }
        }
        out.push('>');
    }
    out.push('(');
    for (i, p) in sig.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&param(view, p));
    }
    out.push(')');
    if sig.is_async {
        out.push_str(" async");
    }
    if let Some(e) = &sig.throws {
        let _ = write!(out, " throws {}", ty(view, e));
    }
    let ret = ty(view, &sig.ret);
    if ret != "Void" {
        let _ = write!(out, " -> {ret}");
    }
    out
}

/// Renders a call of `name` with `sig`'s generics given by `subst`:
/// `decode<User>(_ json: String) -> User`.
#[must_use]
pub fn instantiated(
    view: &ProgramView,
    name: &str,
    sig: &Signature,
    subst: &[(GenId, String)],
) -> String {
    let args: Vec<&str> = subst.iter().map(|(_, t)| t.as_str()).collect();
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| param_with(view, p, subst))
        .collect();
    let mut out = format!("{name}<{}>({})", args.join(", "), params.join(", "));
    if sig.is_async {
        out.push_str(" async");
    }
    if let Some(e) = &sig.throws {
        let _ = write!(out, " throws {}", ty_with(view, e, subst));
    }
    let ret = ty_with(view, &sig.ret, subst);
    if ret != "Void" {
        let _ = write!(out, " -> {ret}");
    }
    out
}

/// The keyword that declares a declaration of `kind`.
#[must_use]
pub fn kind_keyword(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Component => "component",
        DeclarationKind::Page => "page",
        DeclarationKind::Layout => "layout",
        DeclarationKind::Service => "service",
        DeclarationKind::Protocol => "protocol",
        DeclarationKind::Entity => "entity",
        DeclarationKind::Repository => "repository",
        DeclarationKind::Struct => "struct",
        DeclarationKind::Enum => "enum",
        DeclarationKind::Class => "class",
        DeclarationKind::Interface => "interface",
        DeclarationKind::Theme => "theme",
        DeclarationKind::App => "app",
        DeclarationKind::Config => "config",
        DeclarationKind::Migration => "migration",
        DeclarationKind::Function => "func",
        DeclarationKind::Constant => "let",
        DeclarationKind::TestSuite => "test",
        DeclarationKind::Preview => "preview",
    }
}

/// Renders a declaration's header as K source: annotations, visibility,
/// keyword, qualified name, generics, signature or type, `implements`.
#[must_use]
pub fn decl(view: &ProgramView, id: DeclId) -> String {
    let Some(d) = view.decls.get(id.0 as usize) else {
        return String::new();
    };
    let mut out = String::new();
    for a in &d.annotations {
        let _ = writeln!(out, "@{a}");
    }
    if d.is_public {
        out.push_str("public ");
    }
    match d.kind {
        DeclKind::Extension => {
            out.push_str("extend ");
            if let Some(t) = &d.extends {
                out.push_str(&ty(view, t));
            }
            return out;
        }
        DeclKind::Named(kind) => {
            out.push_str(kind_keyword(kind));
            out.push(' ');
        }
    }
    out.push_str(&view.qualified_name(id));
    if let Some(sig) = &d.signature {
        out.push_str(&signature_tail(view, sig));
    } else if let Some(t) = &d.constant_type {
        let _ = write!(out, ": {}", ty(view, t));
    } else if !d.generics.is_empty() {
        out.push('<');
        for (i, g) in d.generics.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            if let Some(info) = view.generics.get(g.0 as usize) {
                out.push_str(&info.name);
            }
        }
        out.push('>');
    }
    if !d.implements.is_empty() {
        out.push_str(" implements ");
        for (i, t) in d.implements.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&ty(view, t));
        }
    }
    out
}

/// Renders a member as K source: annotations, `static`, `let`/`var` and
/// type for fields, `func`/`init`/`rpc` with the signature, `case` with
/// payload. The first line names the owner (`// in shop.Cart`).
#[must_use]
pub fn member(view: &ProgramView, id: MemberId) -> String {
    let Some(m) = view.members.get(id.0 as usize) else {
        return String::new();
    };
    let mut out = String::new();
    for a in &m.annotations {
        let _ = write!(out, "@{a} ");
    }
    if m.is_private {
        out.push_str("private ");
    }
    if m.is_static && m.kind != MemberKind::Case {
        out.push_str("static ");
    }
    match m.kind {
        MemberKind::Field | MemberKind::Computed => {
            out.push_str(if m.is_var { "var " } else { "let " });
            out.push_str(&m.name);
            if let Some(t) = &m.ty {
                let _ = write!(out, ": {}", ty(view, t));
            }
        }
        MemberKind::Func | MemberKind::Rpc => {
            out.push_str(if m.kind == MemberKind::Rpc {
                "rpc "
            } else {
                "func "
            });
            out.push_str(&m.name);
            if let Some(sig) = &m.signature {
                out.push_str(&signature_tail(view, sig));
            }
        }
        MemberKind::Init => {
            out.push_str("init");
            if let Some(sig) = &m.signature {
                out.push_str(&signature_tail(view, sig));
            }
        }
        MemberKind::Case => {
            out.push_str("case ");
            out.push_str(&m.name);
            if let Some(sig) = m.signature.as_ref().filter(|s| !s.params.is_empty()) {
                out.push('(');
                for (i, p) in sig.params.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&param(view, p));
                }
                out.push(')');
            }
        }
        MemberKind::Token => {
            if let Some(c) = &m.category {
                let _ = write!(out, "{c} ");
            }
            out.push_str(&m.name);
        }
        MemberKind::Generated => {
            out.push_str("case ");
            out.push_str(&m.name);
        }
    }
    out
}

/// The display name of a member's owner: the extended type for a member of
/// an extension.
#[must_use]
pub fn owner_name(view: &ProgramView, id: MemberId) -> String {
    let Some(m) = view.members.get(id.0 as usize) else {
        return String::new();
    };
    let owner = view.decl(m.owner);
    match (&owner.kind, &owner.extends) {
        (DeclKind::Extension, Some(t)) => ty(view, t),
        _ => owner.display_name.clone(),
    }
}

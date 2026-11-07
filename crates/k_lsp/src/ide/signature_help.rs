//! Signature help: the signatures of the call around the cursor and the
//! parameter being written.
//!
//! The callee is resolved through the checker's recorded resolution of its
//! name, so a call on a half-typed line still finds its function. A type's
//! initializers are its signatures; a type without one is initialized
//! memberwise, and its stored fields are the parameters. A built-in
//! element lists its forms.

use k_diagnostics::{FileId, TextSize};
use k_sema::stdlib::elements;
use k_sema::view::{DeclId, MemberKind, ProgramView, Resolved, Signature};
use k_sema::{Analysis, Cancelled, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::{is_name, render, syntax};

/// One signature: its label and where each parameter is in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureInfo {
    /// The whole signature (`add(product: Product, quantity: Int = …)`).
    pub label: String,
    /// Each parameter's byte range in `label`, and its label (`None` for
    /// `_`).
    pub params: Vec<(usize, usize, Option<String>)>,
}

/// The signatures of a call and the active ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The candidate signatures.
    pub signatures: Vec<SignatureInfo>,
    /// The index of the signature that matches best.
    pub active_signature: usize,
    /// The index of the parameter being written.
    pub active_parameter: usize,
}

/// The signature help at `offset`, or `None` outside a call's argument
/// list.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn signature_help(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Option<SignatureHelp>, Cancelled> {
    let Some(root) = syntax(analysis, file) else {
        return Ok(None);
    };
    let Some((call, args)) = enclosing_call(&root, offset) else {
        return Ok(None);
    };
    let Some(callee) = call.first_child() else {
        return Ok(None);
    };
    let Some(name) = callee_name(&callee) else {
        return Ok(None);
    };
    let view = analysis.program()?;
    let resolution = analysis.resolve_at(file, name.text_range())?;
    let signatures = match &resolution {
        Some(Resolution::Element { name }) => element_signatures(name),
        Some(r) => match view.resolve(r) {
            Some(Resolved::Decl(d)) => decl_signatures(view, d),
            Some(Resolved::Member(m)) => view
                .member(m)
                .signature
                .as_ref()
                .map(|s| vec![from_signature(view, &view.member(m).name, s)])
                .unwrap_or_default(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };
    if signatures.is_empty() {
        return Ok(None);
    }
    let (index, label) = current_argument(&args, offset);
    let pick = |sig: &SignatureInfo| -> Option<usize> {
        match &label {
            Some(l) => sig
                .params
                .iter()
                .position(|p| p.2.as_deref() == Some(l.as_str())),
            None => (index < sig.params.len()).then_some(index),
        }
    };
    let active_signature = signatures
        .iter()
        .position(|s| pick(s).is_some())
        .unwrap_or(0);
    let active_parameter = pick(&signatures[active_signature]).unwrap_or(index);
    Ok(Some(SignatureHelp {
        signatures,
        active_signature,
        active_parameter,
    }))
}

/// The innermost call whose argument list contains `offset` (after its
/// `(`, up to and including its `)`), and the argument list.
fn enclosing_call(root: &SyntaxNode, offset: TextSize) -> Option<(SyntaxNode, SyntaxNode)> {
    let token = root.token_at_offset(offset).left_biased()?;
    for node in token.parent_ancestors() {
        if node.kind() != SyntaxKind::ArgList {
            continue;
        }
        let open = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::LParen)?;
        let close = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::RParen);
        let inside = offset >= open.text_range().end()
            && close
                .as_ref()
                .is_none_or(|c| offset <= c.text_range().start());
        let parent = node.parent()?;
        if inside && parent.kind() == SyntaxKind::CallExpr {
            return Some((parent, node));
        }
    }
    None
}

/// The name token of a callee: the identifier of `f`, the member name of
/// `a.b.f`.
fn callee_name(callee: &SyntaxNode) -> Option<SyntaxToken> {
    match callee.kind() {
        SyntaxKind::NameExpr | SyntaxKind::MemberExpr | SyntaxKind::ImplicitMemberExpr => callee
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| is_name(t.kind()))
            .last(),
        _ => None,
    }
}

/// The index of the argument at `offset` (its commas before the offset)
/// and its label, if it has one.
fn current_argument(args: &SyntaxNode, offset: TextSize) -> (usize, Option<String>) {
    let mut index = 0;
    let mut label = None;
    for el in args.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t)
                if t.kind() == SyntaxKind::Comma && t.text_range().end() <= offset =>
            {
                index += 1;
            }
            rowan::NodeOrToken::Node(arg) if arg.kind() == SyntaxKind::Arg => {
                let range = arg.text_range();
                if range.start() <= offset && offset <= range.end() {
                    let mut tokens = arg
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .filter(|t| !t.kind().is_trivia());
                    if let (Some(first), Some(second)) = (tokens.next(), tokens.next()) {
                        if is_name(first.kind()) && second.kind() == SyntaxKind::Colon {
                            label = Some(first.text().to_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (index, label)
}

/// A signature from the view.
fn from_signature(view: &ProgramView, name: &str, sig: &Signature) -> SignatureInfo {
    let mut label = format!("{name}(");
    let mut params = Vec::new();
    for (i, p) in sig.params.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }
        let start = label.len();
        label.push_str(&render::param(view, p));
        params.push((start, label.len(), p.label.clone()));
    }
    label.push(')');
    if let Some(e) = &sig.throws {
        label.push_str(" throws ");
        label.push_str(&render::ty(view, e));
    }
    let ret = render::ty(view, &sig.ret);
    if ret != "Void" {
        label.push_str(" -> ");
        label.push_str(&ret);
    }
    SignatureInfo { label, params }
}

/// The signatures of calling declaration `d`: a function's signature, a
/// type's initializers, or its memberwise initializer.
fn decl_signatures(view: &ProgramView, d: DeclId) -> Vec<SignatureInfo> {
    let info = view.decl(d);
    if let Some(sig) = &info.signature {
        return vec![from_signature(view, &info.name, sig)];
    }
    let inits: Vec<SignatureInfo> = info
        .members
        .iter()
        .map(|&m| view.member(m))
        .filter(|m| m.kind == MemberKind::Init)
        .filter_map(|m| {
            m.signature
                .as_ref()
                .map(|s| from_signature(view, &info.name, s))
        })
        .collect();
    if !inits.is_empty() {
        return inits;
    }
    // Memberwise: the stored, non-static fields that callers set.
    let mut label = format!("{}(", info.name);
    let mut params = Vec::new();
    let mut first = true;
    for &m in &info.members {
        let member = view.member(m);
        let internal = ["Inject", "State", "Derived", "Load", "Effect"]
            .iter()
            .any(|a| member.has_annotation(a));
        if member.kind != MemberKind::Field || member.is_static || member.is_private || internal {
            continue;
        }
        if !first {
            label.push_str(", ");
        }
        first = false;
        let start = label.len();
        label.push_str(&member.name);
        if let Some(t) = &member.ty {
            label.push_str(": ");
            label.push_str(&render::ty(view, t));
        }
        if member.has_default {
            label.push_str(" = …");
        }
        params.push((start, label.len(), Some(member.name.clone())));
    }
    label.push(')');
    vec![SignatureInfo { label, params }]
}

/// The forms of built-in element `name`.
fn element_signatures(name: &str) -> Vec<SignatureInfo> {
    let Some(def) = elements::element(name) else {
        return Vec::new();
    };
    def.forms
        .iter()
        .map(|form| {
            let mut label = format!("{name}(");
            let mut params = Vec::new();
            for (i, p) in form.iter().enumerate() {
                if i > 0 {
                    label.push_str(", ");
                }
                let start = label.len();
                if p.label.is_empty() || p.label == "_" {
                    label.push_str(&format!("_: {}", p.ty));
                    params.push((start, label.len(), None));
                } else {
                    label.push_str(&format!("{}: {}", p.label, p.ty));
                    params.push((start, label.len(), Some(p.label.to_owned())));
                }
            }
            label.push(')');
            SignatureInfo { label, params }
        })
        .collect()
}

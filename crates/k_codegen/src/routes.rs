//! The route table that `k routes` prints (`docs/design/12-routing.md`,
//! "`routes.lock` and redirects"; `docs/design/27-endpoint-pages.md`).
//!
//! [`route_table`] lists, for a checked program, every URL the app answers:
//! each view page (`GET`), each handler of each endpoint page (one row per
//! method), and each redirect recorded in `routes.lock`. A row carries the
//! page's parameters (path parameters first, then query parameters with
//! their defaults) and the guards of its `requires` clause as written.
//!
//! The table is computed from the same functions code generation uses for
//! the route matchers and the manifest ([`crate::app::template`],
//! [`crate::app::params`]), so it never disagrees with what the server
//! serves.

use k_sema::stdlib::DeclarationKind;
use k_sema::view::DeclId;
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use crate::cx::Gen;
use crate::{CodegenError, Options};

/// What answers a [`Route`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RouteKind {
    /// A page with a `view`, rendered for `GET`.
    Page,
    /// An endpoint page's handler (`@Get`, `@Post`, …).
    Endpoint,
    /// A permanent redirect recorded in `routes.lock` to the page named.
    Redirect,
}

/// One parameter of a page.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RouteParam {
    /// The field name, `slug`.
    pub name: String,
    /// The declared type as written, `String`, `Page`.
    pub ty: String,
    /// The default as written (`.admin`, `"info"`); `None` for a path
    /// parameter.
    pub default: Option<String>,
    /// `true` for a query parameter (one with a default or an optional
    /// type), `false` for a path segment.
    pub query: bool,
}

/// One row of the route table.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Route {
    /// The HTTP method, upper case: `GET` for pages and redirects.
    pub method: String,
    /// The URL template, `/products/{slug}`.
    pub url: String,
    /// The page's declaration name (`ProductDetail`); for a redirect, the
    /// name of the page it redirects to, or the recorded `Page` case when
    /// no page of that name exists any more.
    pub page: String,
    /// What answers the URL.
    pub kind: RouteKind,
    /// The page's parameters; empty for redirects and endpoints.
    pub params: Vec<RouteParam>,
    /// The guards of the page's `requires` clause, as written
    /// (`Authenticated`, `Role(.admin)`, `RateLimit(10.perMinute)`).
    pub guards: Vec<String>,
}

/// The HTTP methods of endpoint handlers, in the order rows print.
const METHODS: [&str; 5] = ["Get", "Post", "Put", "Patch", "Delete"];

/// Computes the route table of the analysed project, sorted by URL, then
/// by method in the order `GET`, `POST`, `PUT`, `PATCH`, `DELETE`.
///
/// # Errors
///
/// [`CodegenError::HasErrors`] when the program has errors (its routes
/// are not well defined), [`CodegenError::Cancelled`] when the analysis
/// was cancelled.
pub fn route_table(analysis: &k_sema::Analysis) -> Result<Vec<Route>, CodegenError> {
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| CodegenError::Cancelled)?;
    let count = diagnostics.iter().filter(|d| d.is_error()).count();
    if count > 0 {
        return Err(CodegenError::HasErrors { count });
    }
    let view = analysis.program().map_err(|_| CodegenError::Cancelled)?;
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut out = Vec::new();
    for p in crate::app::pages(&g) {
        out.push(Route {
            method: "GET".into(),
            url: crate::app::template(&g, p),
            page: g.view.decl(p).name.clone(),
            kind: RouteKind::Page,
            params: params(&g, p),
            guards: guards(&g, p),
        });
    }
    for p in crate::app::endpoints(&g) {
        let url = crate::app::template(&g, p);
        let guards = guards(&g, p);
        let info = g.view.decl(p);
        for method in METHODS {
            if info
                .members
                .iter()
                .any(|&m| g.view.member(m).has_annotation(method))
            {
                out.push(Route {
                    method: method.to_uppercase(),
                    url: url.clone(),
                    page: info.name.clone(),
                    kind: RouteKind::Endpoint,
                    params: Vec::new(),
                    guards: guards.clone(),
                });
            }
        }
    }
    let pages = crate::app::pages(&g);
    for (from, case) in crate::server::lock_redirects(&g) {
        let page = pages
            .iter()
            .map(|&p| g.view.decl(p).name.clone())
            .find(|name| crate::app::case_name(name) == case)
            .unwrap_or(case);
        out.push(Route {
            method: "GET".into(),
            url: from,
            page,
            kind: RouteKind::Redirect,
            params: Vec::new(),
            guards: Vec::new(),
        });
    }
    let rank = |m: &str| {
        METHODS
            .iter()
            .position(|x| x.eq_ignore_ascii_case(m))
            .unwrap_or(METHODS.len())
    };
    out.sort_by(|a, b| {
        (a.url.as_str(), rank(&a.method), a.page.as_str()).cmp(&(
            b.url.as_str(),
            rank(&b.method),
            b.page.as_str(),
        ))
    });
    Ok(out)
}

/// The `@Param` fields of `page`: path parameters in declaration order,
/// then query parameters in declaration order.
fn params(g: &Gen<'_>, page: DeclId) -> Vec<RouteParam> {
    let mut path = Vec::new();
    let mut query = Vec::new();
    for (m, name, _, has_default) in crate::app::params(g, page) {
        let node = g.member_node(m).and_then(ast::FieldDecl::cast);
        let ty = node
            .as_ref()
            .and_then(ast::FieldDecl::ty)
            .map(|t| squash(&t.syntax().text().to_string()))
            .unwrap_or_default();
        let default = node.as_ref().and_then(|f| {
            f.syntax()
                .children()
                .find_map(ast::Expr::cast)
                .map(|e| squash(&e.syntax().text().to_string()))
        });
        let optional = ty.ends_with('?');
        let row = RouteParam {
            name,
            ty,
            default,
            query: has_default || optional,
        };
        if row.query {
            query.push(row);
        } else {
            path.push(row);
        }
    }
    path.extend(query);
    path
}

/// The guards of `page`'s `requires` clause, each as its source text with
/// whitespace runs collapsed.
fn guards(g: &Gen<'_>, page: DeclId) -> Vec<String> {
    let Some(node) = g.decl_node(page) else {
        return Vec::new();
    };
    if g.view.decl(page).named_kind() != Some(DeclarationKind::Page) {
        return Vec::new();
    }
    node.children()
        .filter(|c| c.kind() == SyntaxKind::RequiresClause)
        .flat_map(|clause| {
            clause
                .children()
                .filter(|c| c.kind() == SyntaxKind::Guard)
                .map(|g| squash(&g.text().to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `text` trimmed, with every run of whitespace replaced by one space.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

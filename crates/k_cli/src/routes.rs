//! `k routes`: prints the route table of a project
//! (`docs/design/12-routing.md`, "`routes.lock` and redirects").
//!
//! The table has one row per URL and method: view pages (`GET`), each
//! handler of an endpoint page, and the redirects `routes.lock` records.
//! Columns are `METHOD`, `URL`, `PAGE` (an endpoint or redirect says so in
//! parentheses), `PARAMS` (path parameters as `name: Type`, query
//! parameters as `?name: Type = default`) and `GUARDS` (the `requires`
//! clause as written). Columns are padded to their widest cell plus two
//! spaces; trailing spaces are removed.

use std::process::ExitCode;

use k_codegen::{CodegenError, Route, RouteKind, RouteParam, route_table};

use crate::cli::ProjectArgs;
use crate::project;

/// Runs `k routes`: exit 0 with the table on standard output, 1 when the
/// project has errors, 2 when the directory is not a project.
pub fn run(args: &ProjectArgs) -> ExitCode {
    let project = match project::load(&args.project, "routes") {
        Ok(p) => p,
        Err(code) => return code,
    };
    let analysis = project.host.analysis();
    match route_table(&analysis) {
        Ok(routes) => {
            print!("{}", render(&routes));
            ExitCode::SUCCESS
        }
        Err(CodegenError::HasErrors { count }) => project.report_errors(count),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// The table as text, header first, one line per route.
pub fn render(routes: &[Route]) -> String {
    let mut rows: Vec<[String; 5]> = vec![[
        "METHOD".into(),
        "URL".into(),
        "PAGE".into(),
        "PARAMS".into(),
        "GUARDS".into(),
    ]];
    for r in routes {
        let page = match r.kind {
            RouteKind::Page => r.page.clone(),
            RouteKind::Endpoint => format!("{} (endpoint)", r.page),
            RouteKind::Redirect => format!("{} (redirect)", r.page),
        };
        rows.push([
            r.method.clone(),
            r.url.clone(),
            page,
            r.params.iter().map(param).collect::<Vec<_>>().join(", "),
            r.guards.join(", "),
        ]);
    }
    let mut widths = [0usize; 5];
    for row in &rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for row in &rows {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if i + 1 < row.len() {
                let pad = widths[i] + 2 - cell.chars().count();
                line.extend(std::iter::repeat_n(' ', pad));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// One parameter: `slug: String`, or `?tab: String = "info"` for a query
/// parameter.
fn param(p: &RouteParam) -> String {
    let mut s = String::new();
    if p.query {
        s.push('?');
    }
    s.push_str(&p.name);
    s.push_str(": ");
    s.push_str(&p.ty);
    if let Some(d) = &p.default {
        s.push_str(" = ");
        s.push_str(d);
    }
    s
}

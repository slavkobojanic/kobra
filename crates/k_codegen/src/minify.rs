//! The release-build minifier (`docs/compiler/codegen.md` §17.2): every
//! client JavaScript chunk of a [`crate::Profile::Release`] build goes
//! through oxc (pinned exactly in the workspace's `Cargo.toml`), which
//! compresses it, drops what nothing reaches and gives local and top-level
//! bindings short names, and writes a source map back to the readable
//! chunk the printer produced.
//!
//! Only binding names change: property names, string keys and exported
//! names keep their spelling, so DOM and Web APIs, JSON wire keys, storage
//! keys and the names other chunks import are untouched. A runtime chunk
//! is also **shaken**: exports no other chunk of the app imports stop
//! being exports first, so the minifier removes them and everything only
//! they reach ([`minify`]'s `keep`).
//!
//! The development and test profiles never come here: their output stays
//! the readable, golden-tested text.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, Statement};
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

/// A minified chunk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Minified {
    /// The minified module text, without a `sourceMappingURL` comment.
    pub code: String,
    /// Its source map (v3 JSON): one source, `source_name`, whose content
    /// (embedded) is the readable input.
    pub map: String,
}

/// Minifies the ES module `source`.
///
/// `source_name` is the name the source map gives the input (its content
/// is embedded, so nothing fetches it). With `keep`, every export whose
/// name isn't in it stops being an export before compression (its
/// declaration stays, and goes only if nothing else in the module uses
/// it): this is how a runtime chunk keeps only what the app reaches.
///
/// # Errors
///
/// The text the parser rejected, which is a code generator bug: callers
/// ship the input unminified rather than fail the build.
pub(crate) fn minify(
    source: &str,
    source_name: &str,
    keep: Option<&BTreeSet<String>>,
) -> Result<Minified, String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    if !parsed.diagnostics.is_empty() {
        let first = parsed
            .diagnostics
            .first()
            .map_or_else(|| "the parser gave up".to_owned(), |d| d.to_string());
        return Err(first);
    }
    let mut program = parsed.program;
    if let Some(keep) = keep {
        unexport(&mut program.body, keep);
    }
    let options = MinifierOptions {
        mangle: Some(MangleOptions {
            top_level: Some(true),
            ..MangleOptions::default()
        }),
        compress: Some(CompressOptions::smallest()),
        mangle_properties: None,
    };
    let minified = Minifier::new(options).minify(&allocator, &mut program);
    let codegen = CodegenOptions {
        source_map_path: Some(std::path::PathBuf::from(source_name)),
        ..CodegenOptions::minify()
    };
    let printed = Codegen::new()
        .with_options(codegen)
        .with_scoping(minified.scoping)
        .build(&program);
    let map = printed
        .map
        .map(|m| m.to_json_string())
        .ok_or_else(|| "no source map was produced".to_owned())?;
    let mut code = printed.code;
    if !code.ends_with('\n') {
        code.push('\n');
    }
    Ok(Minified { code, map })
}

/// Turns every top-level export of `body` whose name isn't in `keep` into
/// a plain declaration (`export function f` becomes `function f`), and
/// drops such names from `export { … }` lists.
fn unexport(body: &mut oxc_allocator::Vec<'_, Statement<'_>>, keep: &BTreeSet<String>) {
    let kept = |name: &str| keep.contains(name);
    let mut i = 0;
    while i < body.len() {
        match &mut body[i] {
            Statement::ExportDeclaration(export) => {
                let names = declared_names(&export.declaration);
                if !names.iter().any(|n| kept(n))
                    && let Statement::ExportDeclaration(export) = body.remove(i)
                {
                    body.insert(i, Statement::from(export.unbox().declaration));
                }
            }
            Statement::ExportNamedDeclaration(list) => {
                list.specifiers.retain(|s| kept(s.exported.name().as_str()));
                if list.specifiers.is_empty() {
                    body.remove(i);
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// The binding names a declaration introduces.
fn declared_names(d: &Declaration<'_>) -> Vec<String> {
    match d {
        Declaration::VariableDeclaration(v) => v
            .declarations
            .iter()
            .flat_map(|d| d.id.get_binding_identifiers())
            .map(|b| b.name.to_string())
            .collect(),
        Declaration::FunctionDeclaration(f) => f.id.iter().map(|b| b.name.to_string()).collect(),
        Declaration::ClassDeclaration(c) => c.id.iter().map(|b| b.name.to_string()).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_and_maps_back_to_the_readable_chunk() {
        let src = "export function greet(personName) {\n  const greeting = \"hello \" + personName;\n  return greeting;\n}\n";
        let out = minify(src, "app.js", None).expect("valid input");
        assert!(out.code.len() < src.len(), "{}", out.code);
        assert!(out.code.contains("export"), "the export keeps its name");
        assert!(out.code.contains("greet"), "{}", out.code);
        assert!(!out.code.contains("personName"), "locals are renamed");
        let map: serde_json::Value = serde_json::from_str(&out.map).expect("JSON map");
        assert_eq!(map["version"], 3);
        assert_eq!(map["sources"][0], "app.js");
        assert_eq!(map["sourcesContent"][0], src);
    }

    #[test]
    fn keep_drops_the_exports_nobody_imports_and_what_only_they_use() {
        let src = "function helper() { return 41; }\nexport function used() { return 1; }\nexport function unused() { return helper() + 1; }\nconst a = 1, b = 2;\nexport { a, b };\n";
        let keep: BTreeSet<String> = ["used".to_owned(), "b".to_owned()].into();
        let out = minify(src, "runtime.js", Some(&keep)).expect("valid input");
        assert!(out.code.contains("used"), "{}", out.code);
        assert!(!out.code.contains("unused"), "{}", out.code);
        assert!(
            !out.code.contains("41"),
            "helper went with it: {}",
            out.code
        );
        assert!(
            out.code.contains(" as b") || out.code.contains("{b}"),
            "{}",
            out.code
        );
    }

    #[test]
    fn keeps_property_names_and_string_keys() {
        let src = "export function enc(v) {\n  return { \"isDone\": v.isDone, title: v.title, node: v.el.firstChild.nextSibling };\n}\n";
        let out = minify(src, "shared.js", None).expect("valid input");
        for name in ["isDone", "title", "firstChild", "nextSibling"] {
            assert!(out.code.contains(name), "{name}: {}", out.code);
        }
    }

    #[test]
    fn rejects_text_that_does_not_parse() {
        assert!(minify("export function (", "bad.js", None).is_err());
    }
}

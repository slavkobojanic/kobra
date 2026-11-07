//! Consistency tests of the embedded standard library and its tables.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use k_diagnostics::FileId;
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use super::annotations::{ANNOTATIONS, annotation};
use super::css::{self, DATA_TYPES, PROPERTIES};
use super::elements::{self, ELEMENTS, HTML_NAMES, UNIVERSAL_ATTRIBUTES};
use super::*;

fn stdlib_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            out.push(
                relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

#[test]
fn embedded_files_match_the_directory() {
    let mut on_disk = Vec::new();
    collect(&stdlib_dir(), &stdlib_dir(), &mut on_disk);
    on_disk.sort();
    let listed: Vec<String> = files::FILES.iter().map(|(p, _)| (*p).to_owned()).collect();
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(listed, sorted, "files.rs must list the files in byte order");
    assert_eq!(
        listed, on_disk,
        "files.rs must list exactly the files below crates/k_sema/stdlib"
    );
    for path in &on_disk {
        assert!(path.ends_with(".k"), "{path} is not a .k file");
    }
}

#[test]
fn every_file_parses_without_diagnostics() {
    let stdlib = Stdlib::get();
    assert_eq!(
        stdlib.files().len(),
        files::FILES.len(),
        "every file is in a module"
    );
    for file in stdlib.files() {
        let diagnostics = file.parse.diagnostics();
        assert!(
            diagnostics.is_empty(),
            "stdlib/{} has syntax errors: {diagnostics:#?}",
            file.path
        );
        assert_eq!(file.parse.syntax().text(), file.text);
    }
}

#[test]
fn every_module_is_below_k() {
    for module in Stdlib::get().modules() {
        assert!(module.is_stdlib(), "{module} is not below `k`");
    }
    let modules: Vec<&str> = Stdlib::get().modules().iter().map(|m| m.as_str()).collect();
    for expected in [
        "k.auth",
        "k.charts",
        "k.core",
        "k.crypto",
        "k.db",
        "k.device",
        "k.drag",
        "k.encoding",
        "k.http",
        "k.internal",
        "k.observe",
        "k.rpc",
        "k.server",
        "k.storage",
        "k.test",
        "k.text",
        "k.time",
        "k.ui",
        "k.validation",
    ] {
        assert!(modules.contains(&expected), "missing module {expected}");
    }
}

/// The stdlib follows the language's file rule: at most one public
/// declaration per file, named like the file (docs/design/16-modules.md).
#[test]
fn files_follow_the_one_public_declaration_rule() {
    let stdlib = Stdlib::get();
    for (index, file) in stdlib.files().iter().enumerate() {
        let stem = file
            .path
            .rsplit('/')
            .next()
            .and_then(|n| n.strip_suffix(".k"))
            .unwrap();
        let public: Vec<&StdlibDeclaration> = stdlib
            .declarations()
            .iter()
            .filter(|d| d.file == index && d.is_public)
            .collect();
        assert!(
            public.len() <= 1,
            "stdlib/{} has {} public declarations",
            file.path,
            public.len()
        );
        assert_eq!(
            public.first().map(|d| d.name.as_str()),
            Some(stem),
            "stdlib/{} must declare `public … {stem}`",
            file.path
        );
    }
}

#[test]
fn declarations_are_unique_and_found_by_qualified_name() {
    let stdlib = Stdlib::get();
    let mut seen = BTreeSet::new();
    for declaration in stdlib.declarations() {
        let qualified = declaration.qualified_name();
        assert!(
            seen.insert(qualified.clone()),
            "{qualified} is declared twice"
        );
        assert_eq!(stdlib.lookup(&qualified), Some(declaration));
    }
    assert!(stdlib.lookup("k.http.Nope").is_none());
    let client = stdlib.lookup("k.http.HttpClient").unwrap();
    assert_eq!(client.kind, DeclarationKind::Interface);
    assert!(client.has_annotation("Platform"));
    let text = &stdlib.files()[client.file].text[client.name_range];
    assert_eq!(text, "HttpClient");
}

/// Every public declaration, and every member of a public type, carries a
/// `///` comment: the stdlib is documented to JDK quality.
#[test]
fn public_declarations_and_their_members_are_documented() {
    let mut missing = Vec::new();
    for file in Stdlib::get().files() {
        let root = ast::SourceFile::cast(file.parse.syntax()).unwrap();
        for item in root.items() {
            let node = item.syntax();
            let is_public = node
                .children_with_tokens()
                .any(|e| e.kind() == SyntaxKind::PublicKw);
            if !is_public {
                continue;
            }
            if !has_doc(node) {
                missing.push(format!("stdlib/{}: a public declaration", file.path));
            }
            if matches!(item, ast::Item::ThemeDecl(_)) {
                continue;
            }
            for list in node.children().filter_map(ast::MemberList::cast) {
                for member in list.members() {
                    if !has_doc(member.syntax()) {
                        let text = member.syntax().text().to_string();
                        let first = text.trim().lines().next().unwrap_or("").to_owned();
                        missing.push(format!("stdlib/{}: `{first}`", file.path));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "undocumented: {missing:#?}");
}

fn has_doc(node: &k_syntax::SyntaxNode) -> bool {
    node.children_with_tokens()
        .take_while(|e| e.kind().is_trivia() || e.kind() == SyntaxKind::Annotation)
        .any(|e| e.kind() == SyntaxKind::DocComment)
        || node
            .first_token()
            .is_some_and(|t| t.kind() == SyntaxKind::DocComment)
}

#[test]
fn the_prelude_holds_the_core_types_and_the_reexports() {
    let stdlib = Stdlib::get();
    let prelude: BTreeSet<&str> = stdlib.prelude().iter().map(|(n, _)| *n).collect();
    // docs/design/20-standard-library.md, "Core (no import)", and the
    // framework names the design uses without an import.
    for name in [
        "String",
        "Int",
        "Double",
        "Decimal",
        "Bool",
        "Bytes",
        "List",
        "Map",
        "Set",
        "Optional",
        "Result",
        "LoadState",
        "Stream",
        "Length",
        "Duration",
        "Percent",
        "Angle",
        "Date",
        "Color",
        "Url",
        "Money",
        "Currency",
        "Secret",
        "Void",
        "Error",
        "Codable",
        "Equatable",
        "Hashable",
        "Copyable",
        "View",
        "Event",
        "Binding",
        "Classes",
        "Page",
        "Endpoint",
        "ThemeVariant",
        "Spacing",
        "Radius",
        "FontSize",
        "Font",
        "State",
        "Prop",
        "Derived",
        "Effect",
        "Load",
        "Emits",
        "Slot",
        "Stylable",
        "Inject",
        "Scope",
        "Param",
        "Layout",
        "NoLayout",
        "SignInPage",
        "Rendering",
        "Get",
        "Post",
        "Env",
        "Id",
        "Unique",
        "Index",
        "Relation",
        "Version",
        "JsonKey",
        "JobName",
        "Retry",
        "Schedule",
        "untracked",
        "onCleanup",
        "global",
    ] {
        assert!(prelude.contains(name), "`{name}` is not in the prelude");
    }
    for qualified in PRELUDE_REEXPORTS {
        assert!(
            stdlib.lookup(qualified).is_some(),
            "{qualified} doesn't exist"
        );
    }
    // Names that need an import are not in the prelude.
    for name in [
        "Uuid",
        "EmailAddress",
        "Clock",
        "HttpClient",
        "Form",
        "Native",
    ] {
        assert!(!prelude.contains(name), "`{name}` must need an import");
    }
}

#[test]
fn the_annotation_table_matches_the_annotation_declarations() {
    let stdlib = Stdlib::get();
    let names: Vec<&str> = ANNOTATIONS.iter().map(|a| a.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "ANNOTATIONS must be sorted and unique");
    for def in ANNOTATIONS {
        let declaration = stdlib
            .lookup(def.name)
            .unwrap_or_else(|| panic!("{} has no stdlib declaration", def.name));
        assert!(
            declaration.has_annotation("Annotation"),
            "{} is not marked @Annotation",
            def.name
        );
        assert!(!def.targets.is_empty());
        assert!(
            Path::new("../../docs/design").join(def.design).exists()
                || PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../docs/design")
                    .join(def.design)
                    .exists(),
            "{}: no design document {}",
            def.name,
            def.design
        );
    }
    for declaration in stdlib.declarations() {
        if declaration.has_annotation("Annotation")
            && declaration.module.as_str() != INTERNAL_MODULE
        {
            assert!(
                annotation(&declaration.qualified_name()).is_some(),
                "{} is an annotation without a table entry",
                declaration.qualified_name()
            );
        }
    }
}

/// Collects the single-segment type names a K type string mentions.
fn type_names(ty: &str) -> Vec<String> {
    let source = format!("let x: {ty}\n");
    let parse = k_syntax::parse(FileId::new(0), &source);
    assert!(parse.diagnostics().is_empty(), "`{ty}` is not a valid type");
    parse
        .syntax()
        .descendants()
        .filter_map(ast::PathType::cast)
        .filter_map(|p| {
            p.syntax()
                .children_with_tokens()
                .find(|e| e.kind() == SyntaxKind::Ident)
                .and_then(|e| e.into_token())
                .map(|t| t.text().to_owned())
        })
        .collect()
}

fn known_type(name: &str, extra: &[&str]) -> bool {
    let stdlib = Stdlib::get();
    name == "T"
        || stdlib.prelude().iter().any(|(n, _)| *n == name)
        || extra.iter().any(|q| q.rsplit('.').next() == Some(name))
}

#[test]
fn element_table_is_sorted_and_names_existing_types() {
    let names: Vec<&str> = ELEMENTS.iter().map(|e| e.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "ELEMENTS must be sorted and unique");
    for scope in elements::TYPE_SCOPE {
        assert!(
            Stdlib::get().lookup(scope).is_some(),
            "{scope} doesn't exist"
        );
    }
    let params = ELEMENTS
        .iter()
        .flat_map(|e| e.forms.iter().flat_map(|f| f.iter()).chain(e.attributes))
        .chain(UNIVERSAL_ATTRIBUTES);
    for param in params {
        for name in type_names(param.ty) {
            assert!(
                known_type(&name, elements::TYPE_SCOPE),
                "`{name}` in `{}: {}` is not a known type",
                param.label,
                param.ty
            );
        }
    }
    for element in ELEMENTS {
        for event in element.events {
            for name in type_names(event.payload) {
                assert!(known_type(&name, &[]), "event payload `{name}`");
            }
        }
        if let elements::Content::Only(children) = element.content {
            for child in children {
                assert!(
                    elements::element(child).is_some(),
                    "{child} isn't an element"
                );
            }
        }
        let labels: Vec<&str> = element.attributes.iter().map(|a| a.label).collect();
        let unique: BTreeSet<&str> = labels.iter().copied().collect();
        assert_eq!(
            labels.len(),
            unique.len(),
            "{}: duplicate attribute",
            element.name
        );
        assert_eq!(elements::element(element.name), Some(element));
    }
    // docs/design/05-views.md, "Built-in element names".
    for name in [
        "link",
        "image",
        "box",
        "text",
        "paragraph",
        "heading",
        "list",
        "numberedList",
        "item",
        "lineBreak",
        "divider",
        "emphasis",
        "textField",
        "passwordField",
        "numberField",
        "checkbox",
        "dateField",
        "dropdown",
        "option",
        "textArea",
        "table",
        "row",
        "headerCell",
        "cell",
        "codeBlock",
        "code",
        "definitionList",
        "term",
        "definition",
        "superscript",
        "subscript",
        "button",
        "form",
        "label",
        "nav",
        "main",
        "section",
        "article",
        "aside",
        "header",
        "footer",
        "figure",
        "caption",
        "video",
        "audio",
        "canvas",
    ] {
        assert!(elements::element(name).is_some(), "missing element {name}");
    }
    let image = elements::element("image").unwrap();
    assert_eq!(
        image.forms[0].len(),
        2,
        "image requires source and description"
    );
    assert!(
        elements::element("button")
            .unwrap()
            .attribute("pressed")
            .is_some()
    );
    assert!(
        elements::element("button")
            .unwrap()
            .attribute("label")
            .is_none()
    );
    assert!(
        elements::element("box")
            .unwrap()
            .attribute("class")
            .is_some()
    );
    assert!(
        elements::element("box")
            .unwrap()
            .attribute("pressed")
            .is_none()
    );
    assert_eq!(
        elements::element("form")
            .unwrap()
            .event("submit")
            .map(|e| e.payload),
        Some("Void")
    );
    assert!(elements::element("div").is_none());
}

#[test]
fn html_names_are_sorted_and_suggest_existing_elements() {
    let names: Vec<&str> = HTML_NAMES.iter().map(|(h, _)| *h).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "HTML_NAMES must be sorted and unique");
    for (html, k) in HTML_NAMES {
        assert!(
            elements::element(k).is_some(),
            "{html} suggests unknown {k}"
        );
        assert!(
            elements::element(html).is_none(),
            "{html} is also a K element"
        );
    }
    assert_eq!(elements::html_suggestion("a"), Some("link"));
    assert_eq!(elements::html_suggestion("button"), None);
}

#[test]
fn css_table_is_sorted_and_consistent() {
    let names: Vec<&str> = PROPERTIES.iter().map(|p| p.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "PROPERTIES must be sorted and unique");
    let data: Vec<&str> = DATA_TYPES.iter().map(|d| d.name).collect();
    let mut sorted_data = data.clone();
    sorted_data.sort_unstable();
    assert_eq!(data, sorted_data, "DATA_TYPES must be sorted");
    for property in PROPERTIES {
        assert_eq!(
            property.css,
            css::kebab_case(property.name),
            "{}",
            property.name
        );
        let mut rest = property.syntax;
        while let Some(start) = rest.find('<') {
            let end = rest[start..].find('>').unwrap() + start + 1;
            let name = &rest[start..end];
            assert!(
                css::data_type(name).is_some(),
                "{}: unknown {name}",
                property.name
            );
            rest = &rest[end..];
        }
        for ty in property.types {
            assert!(
                *ty == "Keyframes" || known_type(ty, &[]),
                "{}: `{ty}` is not a prelude type",
                property.name
            );
        }
        assert_eq!(css::property_named(property.name), Some(property));
    }
    for data_type in DATA_TYPES {
        for ty in data_type.types {
            assert!(
                *ty == "Keyframes" || known_type(ty, &[]),
                "{}: {ty}",
                data_type.name
            );
        }
    }
    assert_eq!(css::kebab_case("borderLeft"), "border-left");
    assert!(
        css::property_named("padding")
            .unwrap()
            .types
            .contains(&"Spacing")
    );
    assert!(
        css::property_named("animationName")
            .unwrap()
            .types
            .contains(&"Keyframes")
    );
    assert!(css::property_named("potato").is_none());
}

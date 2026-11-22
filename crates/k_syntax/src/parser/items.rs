//! Files, imports, declarations, members and the clauses they share
//! (grammar.md §7–§10 and §17).

use k_diagnostics::{Applicability, Diagnostic, Fix, TextEdit, TextRange, TextSize, codes};

use super::engine::{CompletedMarker, EOF, Marker, Parser};
use super::{expressions, statements, styles, types};
use crate::kind::SyntaxKind as K;

/// Where a declaration appears.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Level {
    /// Directly in the file.
    Top,
    /// Inside a `MemberList`.
    Member,
}

// --------------------------------------------------------------- predicates

/// Returns `true` for tokens that may serve as a name after `.`, as a label
/// or as a path segment: identifiers and every keyword.
pub(crate) fn is_name_like(kind: K) -> bool {
    kind.is_name_like()
}

/// Declaration words that are contextual keywords followed by a name.
fn is_decl_word(text: &str) -> bool {
    matches!(
        text,
        "component"
            | "page"
            | "layout"
            | "service"
            | "protocol"
            | "entity"
            | "repository"
            | "theme"
            | "app"
            | "config"
            | "migration"
    )
}

/// Returns `true` if the current token starts a top-level item.
pub(crate) fn at_item_start(p: &Parser<'_>) -> bool {
    match p.current() {
        K::At
        | K::PublicKw
        | K::PrivateKw
        | K::StaticKw
        | K::StructKw
        | K::EnumKw
        | K::ClassKw
        | K::InterfaceKw
        | K::ExtendKw
        | K::FuncKw
        | K::LetKw
        | K::VarKw => true,
        K::Ident => at_contextual_item(p),
        _ => false,
    }
}

/// `component Name`, `test Name`, `test "case"` and friends.
fn at_contextual_item(p: &Parser<'_>) -> bool {
    let text = p.nth_text(0);
    if is_decl_word(text) {
        return p.nth(1) == K::Ident;
    }
    text == "test" && matches!(p.nth(1), K::Ident | K::StringStart)
}

/// Returns `true` if the current token starts a member of a `MemberList`.
pub(crate) fn at_member_start(p: &Parser<'_>) -> bool {
    match p.current() {
        K::At
        | K::PublicKw
        | K::PrivateKw
        | K::StaticKw
        | K::FuncKw
        | K::LetKw
        | K::VarKw
        | K::InitKw
        | K::RpcKw
        | K::CaseKw
        | K::UseKw => true,
        K::StyleKw => p.nth(1) == K::LBrace,
        K::Ident => at_contextual_member(p),
        _ => false,
    }
}

/// Returns `true` if the current identifier starts a contextual member:
/// a `view`, `loading` or `metadata` block, a `failed` handler, a theme
/// `variant`, a `test`, or a theme token or override (any other word
/// followed by a name or `=`).
fn at_contextual_member(p: &Parser<'_>) -> bool {
    let next = p.nth(1);
    match p.nth_text(0) {
        "view" | "loading" | "metadata" => next == K::LBrace,
        "failed" => matches!(next, K::LParen | K::LBrace),
        "variant" => next == K::Ident,
        "test" => matches!(next, K::Ident | K::StringStart),
        // A theme token (`color primary: ...`) or a theme override
        // (`chartGrid = ...`).
        _ => matches!(next, K::Ident | K::Eq),
    }
}

// --------------------------------------------------------------- separators

/// Reports and consumes a `;` (K0103), which acts as a separator.
pub(crate) fn semicolon(p: &mut Parser<'_>) {
    let range = p.current_range();
    let next_same_line = !p.nth_on_new_line(1) && !matches!(p.nth(1), K::RBrace | EOF);
    let fix = if next_same_line {
        let ws = p.whitespace_after(u32::from(range.end()));
        let indent = p.indentation_at(u32::from(range.start())).to_owned();
        p.fix(
            "Replace `;` with a line break",
            TextRange::new(range.start(), ws.end()),
            format!("\n{indent}"),
        )
    } else {
        p.fix("Delete the `;`", range, "")
    };
    let span = p.span(range);
    p.report(
        Diagnostic::new(
            codes::SEMICOLON,
            span,
            "semicolons are not used in Kobra; end the statement with a line break",
        )
        .with_fix(fix),
    );
    p.bump();
}

/// Checks the separation after an element of a `lines(...)` list: `;`
/// (K0103, consumed), a newline, the closing `close` or the end of file are
/// fine; another element on the same line is K0102. Anything else is left
/// for the list's next iteration to report.
pub(crate) fn separator(p: &mut Parser<'_>, close: K, can_start: fn(&Parser<'_>) -> bool) {
    let mut separated = false;
    while p.at(K::Semicolon) {
        semicolon(p);
        separated = true;
    }
    if separated || p.at(close) || p.at_eof() || p.on_new_line() {
        return;
    }
    // A lexer `Error` token after the element (`a$`) does not start a second
    // element: it is wrapped where it occurs, and K0001 covers it.
    if p.eat_error_tokens() && (p.at(close) || p.at_eof() || p.on_new_line()) {
        return;
    }
    // A token that already has a parser diagnostic (`x.5` reports K0100 at
    // `5`) does not also get K0102: a line break would fix nothing.
    let start = p.current_range().start();
    if p.diagnostics
        .last()
        .is_some_and(|d| d.span.start() == start)
    {
        return;
    }
    if can_start(p) {
        let range = p.first_element_range();
        let ws = p.whitespace_before();
        let indent = p.indentation_at(u32::from(range.start())).to_owned();
        let fix = p.fix("Put it on its own line", ws, format!("\n{indent}"));
        let span = p.span(range);
        p.report(
            Diagnostic::new(
                codes::STATEMENTS_ON_ONE_LINE,
                span,
                "expected a line break before this",
            )
            .with_help("statements, members and declarations each go on their own line")
            .with_fix(fix),
        );
    }
}

// --------------------------------------------------------------------- file

/// `SourceFile ::= lines( Import | item )`
pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    let mut first_item: Option<TextRange> = None;
    loop {
        if p.at_eof() {
            break;
        }
        if p.at(K::Semicolon) {
            semicolon(p);
            continue;
        }
        if p.at(K::ImportKw) {
            let import = import(p);
            let range = import.range(p);
            if let Some(first) = first_item {
                let span = p.span(range);
                let label = p.span(first);
                p.report(
                    Diagnostic::new(
                        codes::IMPORT_AFTER_DECLARATION,
                        span,
                        "imports must come before the first declaration",
                    )
                    .with_label(label, "first declaration is here"),
                );
            }
            separator(p, EOF, at_top_start);
        } else if at_item_start(p) {
            let start = p.current_range().start();
            decl(p, Level::Top);
            if first_item.is_none() {
                let end = TextSize::from(p.prev_end()).max(start);
                first_item = Some(TextRange::new(start, end));
            }
            separator(p, EOF, at_top_start);
        } else if p.eat_error_tokens() {
        } else {
            let span = p.span(p.current_range());
            let message = format!("expected a declaration, found {}", p.found_token());
            p.report_junk(Diagnostic::new(codes::EXPECTED_DECLARATION, span, message).with_help(
                "a file holds imports followed by declarations such as `component`, `struct` or `func`",
            ));
            p.skip_until(at_top_start);
        }
    }
    m.complete(p, K::SourceFile);
}

/// Returns `true` if the current token starts a top-level element: an
/// import, a declaration, or a lexer `Error` token (consumed as an error
/// node).
fn at_top_start(p: &Parser<'_>) -> bool {
    p.at(K::ImportKw) || at_item_start(p) || p.at(K::Error)
}

/// `Import ::= 'import' ( ImportPath | 'proto' StringLit )`
fn import(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    if p.at_word("proto") && p.nth(1) == K::StringStart {
        p.bump_remap(K::ProtoKw);
        expressions::string_lit(p);
    } else if is_name_like(p.current()) && !p.on_new_line() {
        import_path(p);
    } else {
        p.expected_token("an import path");
    }
    m.complete(p, K::Import)
}

/// `ImportPath ::= path_word ( ( '.' | '/' | '-' ) path_word )*` without
/// trivia inside.
fn import_path(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    while matches!(p.current(), K::Dot | K::Slash | K::Minus) && p.joined() {
        p.bump();
        if is_name_like(p.current()) && p.joined() {
            p.bump();
        } else {
            let span = p.span(TextRange::empty(TextSize::from(p.prev_end())));
            p.report(Diagnostic::new(
                codes::EXPECTED_TOKEN,
                span,
                "expected a path segment after the separator",
            ));
            break;
        }
    }
    m.complete(p, K::ImportPath);
}

// ------------------------------------------------------------- declarations

/// Facts about a declaration prefix.
struct Prefix {
    /// Annotations or modifiers were written.
    any: bool,
}

/// `prefix ::= Annotation* ( 'public' | 'private' )? 'static'?`
fn prefix(p: &mut Parser<'_>) -> Prefix {
    let mut any = false;
    let mut first_modifier: Option<TextRange> = None;
    let mut visibility = false;
    let mut is_static = false;
    loop {
        match p.current() {
            K::At => {
                any = true;
                let annotation = annotation(p);
                if let Some(modifier) = first_modifier {
                    let range = annotation.range(p);
                    misplaced_annotation(p, range, modifier);
                }
            }
            kind @ (K::PublicKw | K::PrivateKw | K::StaticKw) => {
                any = true;
                let range = p.current_range();
                let duplicate = if kind == K::StaticKw {
                    std::mem::replace(&mut is_static, true)
                } else {
                    std::mem::replace(&mut visibility, true)
                };
                if duplicate {
                    let ws = p.whitespace_after(u32::from(range.end()));
                    let fix = p.fix(
                        format!("Delete the second `{}`", p.nth_text(0)),
                        TextRange::new(range.start(), ws.end()),
                        "",
                    );
                    let span = p.span(range);
                    let message = format!("duplicate modifier `{}`", p.nth_text(0));
                    p.report(
                        Diagnostic::new(codes::DUPLICATE_MODIFIER, span, message).with_fix(fix),
                    );
                }
                first_modifier.get_or_insert(range);
                p.bump();
            }
            _ => break,
        }
    }
    Prefix { any }
}

/// K0121 with a fix that moves the annotation before the first modifier.
fn misplaced_annotation(p: &mut Parser<'_>, range: TextRange, modifier: TextRange) {
    let text = p
        .text()
        .get(usize::from(range.start())..usize::from(range.end()))
        .unwrap_or("")
        .to_owned();
    // The whitespace after the annotation goes with it, unless nothing
    // separates the annotation from what precedes it (`public@Inject let`):
    // then that whitespace is what keeps the modifier and the next word
    // apart.
    let spaced_before = usize::from(range.start())
        .checked_sub(1)
        .and_then(|i| p.text().as_bytes().get(i))
        .is_none_or(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
    let end = if spaced_before {
        p.whitespace_after(u32::from(range.end())).end()
    } else {
        range.end()
    };
    let fix = Fix {
        message: "Move the annotation before the modifiers".to_owned(),
        edits: vec![
            TextEdit {
                span: p.span(TextRange::empty(modifier.start())),
                replacement: format!("{text} "),
            },
            TextEdit {
                span: p.span(TextRange::new(range.start(), end)),
                replacement: String::new(),
            },
        ],
        applicability: Applicability::MachineApplicable,
    };
    let span = p.span(range);
    p.report(
        Diagnostic::new(
            codes::ANNOTATION_ORDER,
            span,
            "annotations must come before `public`, `private` and `static`",
        )
        .with_fix(fix),
    );
}

/// `Annotation ::= '@' IDENT ArgList?`
pub(crate) fn annotation(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    if is_annotation_name(p.current()) {
        p.bump();
        if p.at(K::LParen) && !p.on_new_line() {
            expressions::arg_list(p);
        }
    } else {
        p.expected_token("an annotation name");
    }
    m.complete(p, K::Annotation)
}

/// Returns `true` if `kind` can name an annotation: any word except the
/// keywords that start a declaration, so `@ let x` reads as a missing name
/// followed by a field.
fn is_annotation_name(kind: K) -> bool {
    is_name_like(kind)
        && !matches!(
            kind,
            K::LetKw
                | K::VarKw
                | K::FuncKw
                | K::StructKw
                | K::EnumKw
                | K::ClassKw
                | K::InterfaceKw
                | K::ExtendKw
                | K::InitKw
                | K::RpcKw
                | K::CaseKw
                | K::UseKw
                | K::ImportKw
                | K::PublicKw
                | K::PrivateKw
                | K::StaticKw
                | K::StyleKw
        )
}

/// Parses one item (at [`Level::Top`]) or member (at [`Level::Member`]),
/// starting at its prefix. The caller has checked [`at_item_start`] or
/// [`at_member_start`].
pub(crate) fn decl(p: &mut Parser<'_>, level: Level) {
    let m = p.start();
    let prefix = prefix(p);
    let member = level == Level::Member;
    match p.current() {
        K::StructKw => type_decl(p, m, K::StructDecl),
        K::EnumKw => type_decl(p, m, K::EnumDecl),
        K::ClassKw => type_decl(p, m, K::ClassDecl),
        K::InterfaceKw => type_decl(p, m, K::InterfaceDecl),
        K::ExtendKw => extend_decl(p, m),
        K::FuncKw => func_decl(p, m),
        K::LetKw | K::VarKw => field_decl(p, m, level),
        K::InitKw if member => init_decl(p, m),
        K::RpcKw if member => rpc_decl(p, m),
        K::CaseKw if member => enum_case(p, m),
        K::UseKw if member => use_decl(p, m),
        K::StyleKw if member && p.nth(1) == K::LBrace => styles::style_block(p, m),
        K::Ident if at_contextual_item(p) || (member && at_contextual_member(p)) => {
            contextual_decl(p, m, level);
        }
        _ => {
            if prefix.any {
                p.expected_token("a declaration after the annotations and modifiers");
                m.complete(p, K::ErrorNode);
            } else {
                // Unreachable through the callers' checks; keep progress.
                p.expected_token("a declaration");
                m.abandon(p);
            }
        }
    }
}

/// Parses a declaration introduced by a contextual keyword (`component`,
/// `page`, `service`, `test`, ...) into the node started by `m`, remapping
/// the identifier to its keyword kind.
///
/// At [`Level::Member`] the member-only forms of
/// [`member_block_or_token`] are also accepted; at [`Level::Top`] any
/// other word reports an expected declaration and abandons `m`.
fn contextual_decl(p: &mut Parser<'_>, m: Marker, level: Level) {
    let text = p.nth_text(0);
    let next = p.nth(1);
    match text {
        "component" if next == K::Ident => {
            p.bump_remap(K::ComponentKw);
            decl_name(p);
            if p.at(K::Lt) {
                types::generic_param_list(p);
            }
            body(p, m, K::ComponentDecl);
        }
        "page" if next == K::Ident => {
            p.bump_remap(K::PageKw);
            decl_name(p);
            if p.at(K::RequiresKw) {
                requires_clause(p);
            }
            if p.at_word("at") {
                at_clause(p);
            }
            body(p, m, K::PageDecl);
        }
        "layout" | "protocol" if next == K::Ident => {
            let (kw, node) = if text == "layout" {
                (K::LayoutKw, K::LayoutDecl)
            } else {
                (K::ProtocolKw, K::ProtocolDecl)
            };
            p.bump_remap(kw);
            decl_name(p);
            // A generic protocol parses, so sema can say why it can't be
            // one (K5514).
            if kw == K::ProtocolKw && p.at(K::Lt) {
                types::generic_param_list(p);
            }
            if p.at(K::RequiresKw) {
                requires_clause(p);
            }
            body(p, m, node);
        }
        "service" | "entity" if next == K::Ident => {
            let (kw, node) = if text == "service" {
                (K::ServiceKw, K::ServiceDecl)
            } else {
                (K::EntityKw, K::EntityDecl)
            };
            p.bump_remap(kw);
            decl_name(p);
            if p.at(K::ImplementsKw) {
                implements_clause(p);
            }
            body(p, m, node);
        }
        "repository" if next == K::Ident => {
            p.bump_remap(K::RepositoryKw);
            decl_name(p);
            if p.at(K::ForKw) {
                for_clause(p);
            } else {
                p.expected_token("`for` and the entity type");
            }
            body(p, m, K::RepositoryDecl);
        }
        "config" if next == K::Ident => {
            // A config may implement interfaces (`RemoteService`,
            // 40-workspaces-and-services.md).
            p.bump_remap(K::ConfigKw);
            decl_name(p);
            if p.at(K::ImplementsKw) {
                implements_clause(p);
            }
            body(p, m, K::ConfigDecl);
        }
        "theme" | "app" | "migration" if next == K::Ident => {
            let (kw, node) = match text {
                "theme" => (K::ThemeKw, K::ThemeDecl),
                "app" => (K::AppKw, K::AppDecl),
                _ => (K::MigrationKw, K::MigrationDecl),
            };
            p.bump_remap(kw);
            decl_name(p);
            body(p, m, node);
        }
        "test" if next == K::Ident => {
            p.bump_remap(K::TestKw);
            decl_name(p);
            body(p, m, K::TestSuiteDecl);
        }
        "test" if next == K::StringStart => {
            p.bump_remap(K::TestKw);
            expressions::string_lit(p);
            required_block(p);
            m.complete(p, K::TestCaseDecl);
        }
        _ if level == Level::Member => member_block_or_token(p, m, text, next),
        _ => {
            p.expected_token("a declaration");
            m.abandon(p);
        }
    }
}

/// Member-only contextual declarations: blocks, theme variants and tokens.
fn member_block_or_token(p: &mut Parser<'_>, m: Marker, text: &str, next: K) {
    match text {
        "view" | "loading" | "metadata" if next == K::LBrace => {
            let (kw, node) = match text {
                "view" => (K::ViewKw, K::ViewBlock),
                "loading" => (K::LoadingKw, K::LoadingBlock),
                _ => (K::MetadataKw, K::MetadataBlock),
            };
            p.bump_remap(kw);
            statements::block(p);
            m.complete(p, node);
        }
        "failed" if matches!(next, K::LParen | K::LBrace) => failed_block(p, m),
        "variant" if next == K::Ident => theme_variant(p, m),
        _ if next == K::Eq => {
            theme_override(p);
            m.complete(p, K::ThemeOverride);
        }
        _ => theme_token(p, m),
    }
}

/// The name of a declaration: an identifier (or a reserved word, K0108).
///
/// Lexer `Error` tokens before or right after the name (`let ​b`, `let café`)
/// are wrapped in an `ErrorNode` where they occur, without a parser
/// diagnostic: K0001 covers them and parsing continues as if they were not
/// there. An `Error` token where the name belongs stands in for it.
pub(crate) fn decl_name(p: &mut Parser<'_>) -> bool {
    let stand_in = p.eat_error_tokens();
    let named = match p.current() {
        K::Ident if !(stand_in && p.on_new_line()) => {
            p.bump();
            true
        }
        K::AsKw | K::IsKw if !(stand_in && p.on_new_line()) => {
            reserved_word(p);
            p.bump();
            true
        }
        _ if stand_in => return true,
        _ => {
            p.expected_token("a name");
            return false;
        }
    };
    if p.at(K::Error) && !p.on_new_line() {
        p.eat_error_tokens();
    }
    named
}

/// Reports K0108 for the reserved word at the current token.
pub(crate) fn reserved_word(p: &mut Parser<'_>) {
    let span = p.span(p.current_range());
    let message = format!(
        "`{}` is a reserved word and cannot be used as a name",
        p.nth_text(0)
    );
    p.report(Diagnostic::new(codes::RESERVED_WORD, span, message));
}

/// Parses the body and completes the declaration. If the body is missing
/// but a header clause follows (for example `implements` where it is not
/// allowed), the clause is reported and parsed so the body still attaches.
fn body(p: &mut Parser<'_>, m: Marker, kind: K) {
    p.eat_error_tokens();
    if !p.at(K::LBrace) && stray_clause(p) {
        // The clause was reported as "expected `{`"; parse the body if it
        // follows, without a second diagnostic.
        if p.at(K::LBrace) {
            member_list(p);
        }
    } else {
        member_list(p);
    }
    m.complete(p, kind);
}

/// Parses a clause that is not allowed at this point of a header, after
/// reporting it. Returns `true` if one was found.
fn stray_clause(p: &mut Parser<'_>) -> bool {
    let kind = p.current();
    if !matches!(kind, K::ImplementsKw | K::RequiresKw | K::WhereKw) && !p.at_word("at") {
        return false;
    }
    p.expected_token("`{`");
    match kind {
        K::ImplementsKw => implements_clause(p),
        K::RequiresKw => requires_clause(p),
        K::WhereKw => types::where_clause(p),
        _ => at_clause(p),
    }
    p.eat_error_tokens();
    true
}

/// `struct`, `enum`, `class`, `interface`:
/// `kw IDENT GenericParamList? ImplementsClause? WhereClause? MemberList`
fn type_decl(p: &mut Parser<'_>, m: Marker, kind: K) {
    p.bump();
    decl_name(p);
    if p.at(K::Lt) {
        types::generic_param_list(p);
    }
    if p.at(K::ImplementsKw) {
        implements_clause(p);
    }
    if p.at(K::WhereKw) {
        types::where_clause(p);
    }
    p.eat_error_tokens();
    member_list(p);
    m.complete(p, kind);
}

/// `ExtendDecl ::= prefix 'extend' GenericParamList? Type WhereClause? MemberList`
fn extend_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    if p.at(K::Lt) {
        types::generic_param_list(p);
    }
    types::ty(p);
    if p.at(K::WhereKw) {
        types::where_clause(p);
    }
    p.eat_error_tokens();
    member_list(p);
    m.complete(p, K::ExtendDecl);
}

/// `FuncDecl ::= prefix 'func' IDENT GenericParamList? ParamList 'async'?
/// ThrowsClause? ReturnType? WhereClause? Block?`
fn func_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    decl_name(p);
    if p.at(K::Lt) {
        types::generic_param_list(p);
    }
    p.eat_error_tokens();
    if p.at(K::LParen) {
        param_list(p);
    } else {
        p.expected_token("`(`");
    }
    signature_tail(p, false);
    if p.at(K::WhereKw) {
        types::where_clause(p);
    }
    if p.at(K::LBrace) {
        statements::block(p);
    }
    m.complete(p, K::FuncDecl);
}

/// `'async'? ThrowsClause? ReturnType? RequiresClause?` (requires only for
/// rpcs).
fn signature_tail(p: &mut Parser<'_>, requires: bool) {
    p.eat(K::AsyncKw);
    if p.at(K::ThrowsKw) {
        throws_clause(p);
    }
    if p.at(K::Arrow) {
        return_type(p);
    }
    if requires && p.at(K::RequiresKw) {
        requires_clause(p);
    }
    // An rpc's `where` clause parses with its generics (K5514 in sema).
    if requires && p.at(K::WhereKw) {
        types::where_clause(p);
    }
}

/// `FieldDecl ::= prefix ( 'let' | 'var' ) IDENT ( ':' Type )? ( '=' Expr | Block )?`
fn field_decl(p: &mut Parser<'_>, m: Marker, level: Level) {
    if level == Level::Top && p.at(K::VarKw) {
        let range = p.current_range();
        let fix = p
            .fix("Replace `var` with `let`", range, "let")
            .maybe_incorrect();
        let span = p.span(range);
        p.report(
            Diagnostic::new(
                codes::TOP_LEVEL_VAR,
                span,
                "top-level variables must be `let`",
            )
            .with_help("shared mutable state belongs in a service")
            .with_fix(fix),
        );
    }
    p.bump();
    decl_name(p);
    if p.eat(K::Colon) {
        types::ty(p);
    }
    if p.at(K::Eq) {
        p.bump();
        expressions::required_expr(p);
    } else if p.at(K::LBrace) && !p.on_new_line() {
        statements::block(p);
    }
    m.complete(p, K::FieldDecl);
}

/// `InitDecl ::= prefix 'init' ParamList ThrowsClause? Block`
fn init_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    p.eat_error_tokens();
    if p.at(K::LParen) {
        param_list(p);
    } else {
        p.expected_token("`(`");
    }
    if p.at(K::ThrowsKw) {
        throws_clause(p);
    }
    required_block(p);
    m.complete(p, K::InitDecl);
}

/// `RpcDecl ::= prefix 'rpc' IDENT GenericParamList? ParamList 'async'? ThrowsClause?
/// ReturnType? RequiresClause? Block?`
fn rpc_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    decl_name(p);
    // A generic rpc parses, so sema can say why it can't be one (K5514).
    if p.at(K::Lt) {
        types::generic_param_list(p);
    }
    p.eat_error_tokens();
    if p.at(K::LParen) {
        param_list(p);
    } else {
        p.expected_token("`(`");
    }
    signature_tail(p, true);
    if p.at(K::LBrace) {
        statements::block(p);
    }
    m.complete(p, K::RpcDecl);
}

/// `EnumCase ::= prefix 'case' IDENT ( '(' CaseField,* ')' )?`; `case a, b`
/// is K0130 and builds one `EnumCase` per name.
fn enum_case(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    let indent = p
        .indentation_at(u32::from(p.prev_range().start()))
        .to_owned();
    decl_name(p);
    if p.at(K::LParen) && !p.on_new_line() {
        case_fields(p);
    }
    m.complete(p, K::EnumCase);
    let mut diagnostic: Option<usize> = None;
    while p.at(K::Comma) && !p.on_new_line() {
        let comma = p.current_range();
        if !is_name_like(p.nth(1)) || p.nth_on_new_line(1) {
            // A trailing comma (`case a,` at the end of the line): K0130
            // too, fixed by deleting the comma.
            trailing_case_comma(p, comma, diagnostic);
            p.bump();
            break;
        }
        let next_start = p.nth_range(1).start();
        let edit = TextEdit {
            span: p.span(TextRange::new(comma.start(), next_start.max(comma.end()))),
            replacement: format!("\n{indent}case "),
        };
        match diagnostic {
            None => {
                let span = p.span(comma);
                let fix = Fix {
                    message: "Declare each case on its own line".to_owned(),
                    edits: vec![edit],
                    applicability: Applicability::MachineApplicable,
                };
                if p.report(
                    Diagnostic::new(
                        codes::MULTIPLE_ENUM_CASES,
                        span,
                        "each enum case needs its own `case` keyword",
                    )
                    .with_fix(fix),
                ) {
                    diagnostic = Some(p.diagnostics.len() - 1);
                }
            }
            Some(index) => {
                if let Some(fix) = p
                    .diagnostics
                    .get_mut(index)
                    .and_then(|d| d.fixes.first_mut())
                {
                    fix.edits.push(edit);
                }
            }
        }
        p.bump();
        let case = p.start();
        decl_name(p);
        if p.at(K::LParen) && !p.on_new_line() {
            case_fields(p);
        }
        case.complete(p, K::EnumCase);
    }
}

/// Reports K0130 for a comma after the last name of an enum case, or adds
/// its deletion to the K0130 fix already reported for the same `case` (at
/// index `diagnostic`).
fn trailing_case_comma(p: &mut Parser<'_>, comma: TextRange, diagnostic: Option<usize>) {
    let edit = TextEdit {
        span: p.span(comma),
        replacement: String::new(),
    };
    if let Some(fix) = diagnostic
        .and_then(|index| p.diagnostics.get_mut(index))
        .and_then(|d| d.fixes.first_mut())
    {
        fix.edits.push(edit);
        return;
    }
    let span = p.span(comma);
    p.report(
        Diagnostic::new(
            codes::MULTIPLE_ENUM_CASES,
            span,
            "an enum case does not end with a comma",
        )
        .with_fix(Fix {
            message: "Remove the trailing comma".to_owned(),
            edits: vec![edit],
            applicability: Applicability::MachineApplicable,
        }),
    );
}

/// `'(' ( CaseField ( ',' CaseField )* ','? )? ')'` with
/// `CaseField ::= ( IDENT ':' )? Type ( '=' Expr )?`.
fn case_fields(p: &mut Parser<'_>) {
    expressions::delimited(
        p,
        K::RParen,
        "`,` or `)`",
        |p| {
            let m = p.start();
            if p.at(K::Ident) && p.nth(1) == K::Colon {
                p.bump();
                p.bump();
            }
            types::ty(p);
            if p.eat(K::Eq) {
                expressions::required_expr(p);
            }
            m.complete(p, K::CaseField);
        },
        types::can_start_type,
    );
}

/// `UseDecl ::= 'use' ( Type | Expr ) 'for' Type ( 'in' Expr ( ',' Expr )* )?`
///
/// The operand is an `Expr` exactly when [`use_operand_is_expr`] says so
/// (grammar.md §9): a value bound to a config in a test body
/// (`use Config(secret: s) for Config`, `use Config.sample for Config`).
fn use_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    if use_operand_is_expr(p) {
        expressions::required_expr(p);
    } else {
        types::ty(p);
    }
    if p.eat(K::ForKw) {
        types::ty(p);
    } else {
        p.expected_token("`for`");
    }
    if p.at(K::InKw) {
        p.bump();
        loop {
            expressions::required_expr(p);
            if !p.eat(K::Comma) {
                break;
            }
        }
    }
    m.complete(p, K::UseDecl);
}

/// Decides whether the operand of a `UseDecl` is an expression rather than a
/// type (grammar.md §9, "The operand of `use`").
///
/// The operand is an expression when it cannot start a type but can start an
/// expression (`.sample`, a literal), or when it starts with a dotted path of
/// words `a.b.C.d` that
///
/// * is followed on the same line by `(` or `{` (a call: the memberwise
///   initializer `Config(secret: s)`), or
/// * has a segment after its first capitalized segment that is not a
///   capitalized identifier (a member of a type: `Config.sample`).
///
/// Otherwise (`Fake`, `fakes.FakeApi`, `Repo<User>`, `(A)`) it is a type.
/// The scan is bounded by the path it inspects, which the caller then
/// consumes, so parsing stays linear.
fn use_operand_is_expr(p: &Parser<'_>) -> bool {
    match p.peek(0) {
        K::Ident => {}
        K::LParen | K::Error | EOF => return false,
        _ => return expressions::can_start_expr(p),
    }
    let mut n = 0;
    let mut seen_type = false;
    loop {
        let capitalized =
            p.peek(n) == K::Ident && p.nth_text(n).starts_with(|c: char| c.is_ascii_uppercase());
        if seen_type && !capitalized {
            return true;
        }
        seen_type |= capitalized;
        if p.peek(n + 1) == K::Dot && is_name_like(p.peek(n + 2)) && !p.nth_on_new_line(n + 1) {
            n += 2;
            continue;
        }
        let next = p.peek(n + 1);
        return matches!(next, K::LParen | K::LBrace) && !p.nth_on_new_line(n + 1);
    }
}

/// `ThemeToken ::= IDENT IDENT ':' Type '=' Expr`
fn theme_token(p: &mut Parser<'_>, m: Marker) {
    p.bump();
    decl_name(p);
    if p.expect(K::Colon, "`:`") {
        types::ty(p);
    }
    if p.expect(K::Eq, "`=`") {
        expressions::required_expr(p);
    }
    m.complete(p, K::ThemeToken);
}

/// The contents of a `ThemeOverride` (`IDENT '=' Expr`), without its node.
fn theme_override(p: &mut Parser<'_>) {
    p.bump();
    if p.expect(K::Eq, "`=`") {
        expressions::required_expr(p);
    }
}

/// `ThemeVariant ::= 'variant' IDENT ( 'matches' expr_nt )? '{' lines( ThemeOverride ) '}'`
fn theme_variant(p: &mut Parser<'_>, m: Marker) {
    p.bump_remap(K::VariantKw);
    decl_name(p);
    p.eat_error_tokens();
    if p.at_word("matches") {
        // `variant dark matches .prefersDark { ... }`: the condition is an
        // expression without a trailing lambda, since `{` opens the body.
        p.bump_remap(K::MatchesKw);
        if expressions::can_start_expr(p) && !p.at(K::LBrace) {
            expressions::expr_no_lambda(p);
        } else {
            p.expected(
                codes::EXPECTED_EXPRESSION,
                "a condition such as `.prefersDark`",
            );
        }
        p.eat_error_tokens();
    }
    if !p.at(K::LBrace) {
        p.expected_token("`{`");
        m.complete(p, K::ThemeVariant);
        return;
    }
    if !p.enter() {
        m.complete(p, K::ThemeVariant);
        return;
    }
    let open = p.current_range();
    p.bump();
    let saved = (p.newlines, p.no_trailing_lambda);
    p.newlines = true;
    p.no_trailing_lambda = false;
    loop {
        if p.eat(K::RBrace) {
            break;
        }
        if p.at_eof() {
            p.unclosed(open);
            break;
        }
        if p.at(K::Semicolon) {
            semicolon(p);
            continue;
        }
        if p.at(K::Ident) {
            let o = p.start();
            theme_override(p);
            o.complete(p, K::ThemeOverride);
            separator(p, K::RBrace, |p| p.at(K::Ident));
        } else if p.eat_error_tokens() {
        } else {
            p.expected_token("a token override such as `surface = #111`");
            p.skip_until(|p| p.at(K::Ident) || p.at(K::RBrace));
        }
    }
    (p.newlines, p.no_trailing_lambda) = saved;
    p.leave();
    m.complete(p, K::ThemeVariant);
}

/// `FailedBlock ::= 'failed' ( '(' Pattern ')' )? Block`
fn failed_block(p: &mut Parser<'_>, m: Marker) {
    p.bump_remap(K::FailedKw);
    if p.at(K::LParen) {
        let open = p.current_range();
        p.bump();
        if p.at(K::RParen) {
            p.expected(codes::EXPECTED_PATTERN, "a pattern such as `let error`");
        } else {
            super::patterns::pattern(p, false);
        }
        if !p.eat(K::RParen) {
            p.unclosed(open);
        }
    }
    required_block(p);
    m.complete(p, K::FailedBlock);
}

/// Parses a block that the grammar requires, reporting K0100 when it is
/// missing.
pub(crate) fn required_block(p: &mut Parser<'_>) {
    p.eat_error_tokens();
    if p.at(K::LBrace) {
        statements::block(p);
    } else {
        p.expected_token("`{`");
    }
}

/// `MemberList ::= '{' lines( member ) '}'`
pub(crate) fn member_list(p: &mut Parser<'_>) {
    if !p.at(K::LBrace) {
        p.expected_token("`{`");
        return;
    }
    if !p.enter() {
        return;
    }
    let m = p.start();
    let open = p.current_range();
    p.bump();
    let saved = (p.newlines, p.no_trailing_lambda);
    p.newlines = true;
    p.no_trailing_lambda = false;
    loop {
        if p.eat(K::RBrace) {
            break;
        }
        if p.at_eof() {
            p.unclosed(open);
            break;
        }
        if p.at(K::Semicolon) {
            semicolon(p);
            continue;
        }
        if at_member_start(p) {
            let before = p.position();
            decl(p, Level::Member);
            if p.position() == before {
                p.skip_until(|_| true);
            }
            separator(p, K::RBrace, at_member_start);
        } else if p.eat_error_tokens() {
        } else {
            let span = p.span(p.current_range());
            let message = format!("expected a member, found {}", p.found_token());
            p.report_junk(Diagnostic::new(codes::EXPECTED_MEMBER, span, message).with_help(
                "members are fields, functions, `init`, `rpc`, `case`, `use` and `view`, `style`, `loading`, `failed` or `metadata` blocks",
            ));
            p.skip_until(|p| at_member_start(p) || p.at(K::RBrace) || p.at(K::Error));
        }
    }
    (p.newlines, p.no_trailing_lambda) = saved;
    p.leave();
    m.complete(p, K::MemberList);
}

// ------------------------------------------------------------------ clauses

/// `ImplementsClause ::= 'implements' Type ( ',' Type )*`
fn implements_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    loop {
        types::ty(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    m.complete(p, K::ImplementsClause);
}

/// `RequiresClause ::= 'requires' Guard ( ',' Guard )*` with
/// `Guard ::= IDENT ( '.' IDENT )* ArgList?`.
fn requires_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    loop {
        if p.at(K::Ident) {
            let g = p.start();
            p.bump();
            while p.at(K::Dot) && is_name_like(p.nth(1)) {
                p.bump();
                p.bump();
            }
            if p.at(K::LParen) && !p.on_new_line() {
                expressions::arg_list(p);
            }
            g.complete(p, K::Guard);
        } else {
            p.expected_token("a guard such as `Authenticated`");
            break;
        }
        if !p.eat(K::Comma) {
            break;
        }
    }
    m.complete(p, K::RequiresClause);
}

/// `AtClause ::= 'at' StringLit`
fn at_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump_remap(K::AtKw);
    if p.at(K::StringStart) {
        expressions::string_lit(p);
    } else {
        p.expected_token("a URL template string such as `\"/products\"`");
    }
    m.complete(p, K::AtClause);
}

/// `ForClause ::= 'for' Type`
fn for_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    types::ty(p);
    m.complete(p, K::ForClause);
}

/// `ThrowsClause ::= 'throws' Type` (K0129 when the type is missing).
pub(crate) fn throws_clause(p: &mut Parser<'_>) {
    let m = p.start();
    let range = p.current_range();
    p.bump();
    if types::can_start_type(p) {
        types::ty(p);
    } else {
        let span = p.span(range);
        p.report(
            Diagnostic::new(
                codes::THROWS_WITHOUT_TYPE,
                span,
                "`throws` must name the error type",
            )
            .with_help("write the thrown type, for example `throws CartError`"),
        );
    }
    m.complete(p, K::ThrowsClause);
}

/// `ReturnType ::= '->' Type`
pub(crate) fn return_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    types::ty(p);
    m.complete(p, K::ReturnType);
}

/// `ParamList ::= '(' ( Param ( ',' Param )* ','? )? ')'`
fn param_list(p: &mut Parser<'_>) {
    let m = p.start();
    expressions::delimited(p, K::RParen, "`,` or `)`", param, can_start_param);
    m.complete(p, K::ParamList);
}

/// Returns `true` if the current token can start a parameter: an
/// annotation, `_` or a name.
fn can_start_param(p: &Parser<'_>) -> bool {
    let kind = p.current();
    kind == K::At || kind == K::Underscore || is_name_like(kind)
}

/// `Param ::= Annotation* ( name | '_' )? IDENT ':' Type ( '=' Expr )?`
fn param(p: &mut Parser<'_>) {
    let m = p.start();
    while p.at(K::At) {
        annotation(p);
    }
    let first = p.current();
    if (is_name_like(first) || first == K::Underscore) && is_name_like(p.nth(1)) {
        p.bump();
        p.bump();
    } else if is_name_like(first) || first == K::Underscore {
        p.bump();
    } else {
        p.expected_token("a parameter name");
    }
    if p.eat(K::Colon) {
        types::ty(p);
    } else {
        p.expected_token("`:` and the parameter type");
    }
    if p.eat(K::Eq) {
        expressions::required_expr(p);
    }
    m.complete(p, K::Param);
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use k_diagnostics::FileId;

    #[test]
    fn contextual_keywords_need_a_following_name() {
        let parse = parse(FileId::new(0), "component {}\n");
        assert_eq!(parse.diagnostics()[0].code.to_string(), "K0125");
    }
}

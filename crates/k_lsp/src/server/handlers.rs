//! Request handlers: protocol parameters in, protocol results out, the
//! work done by [`crate::ide`] on a [`Snap`]. Every handler runs on a worker
//! thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use k_diagnostics::{Diagnostic as KDiagnostic, FileId, Severity, Span, TextRange, TextSize};
use k_sema::{Analysis, Cancelled};
use lsp_types as lsp;
use lsp_types::Url;

use crate::ide::{self, completion::ItemKind, symbols::SymbolKind};
use crate::line_index::LineIndex;
use crate::workspace::Project;

/// What the client supports that changes the shape of results.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientCaps {
    /// Workspace edits may rename files (`resourceOperations` has
    /// `rename` and `documentChanges` is supported).
    pub rename_files: bool,
    /// Document symbols may be hierarchical.
    pub hierarchical_symbols: bool,
    /// File watchers may be registered dynamically.
    pub watch_files: bool,
}

/// A snapshot of one project, with what converting results needs.
pub struct Snap {
    /// The analysis.
    pub analysis: Analysis,
    /// The project root.
    pub root: PathBuf,
    /// The workspace root, when the project is a member of a workspace.
    pub ws_root: Option<PathBuf>,
    /// The client's capabilities.
    pub caps: ClientCaps,
    /// The versions of the project's open documents, by file.
    pub versions: HashMap<FileId, i32>,
    /// Line indexes built so far.
    indexes: RefCell<HashMap<FileId, LineIndex>>,
}

impl Snap {
    /// Creates a snapshot.
    #[must_use]
    pub fn new(project: &Project, caps: ClientCaps) -> Snap {
        let analysis = project.analysis();
        let versions = analysis
            .files()
            .into_iter()
            .filter_map(|(id, path)| project.version(path).map(|v| (id, v)))
            .collect();
        Snap {
            analysis,
            root: project.root.clone(),
            ws_root: project.workspace_root().map(Path::to_path_buf),
            caps,
            versions,
            indexes: RefCell::new(HashMap::new()),
        }
    }

    /// The URL of a file of the analysis.
    #[must_use]
    pub fn url(&self, file: FileId) -> Option<Url> {
        let path = Project::file_path(&self.root, self.ws_root.as_deref(), &self.analysis, file)?;
        Url::from_file_path(path).ok()
    }

    /// Runs `f` with the line index and text of `file`.
    fn with_index<T>(&self, file: FileId, f: impl FnOnce(&LineIndex, &str) -> T) -> Option<T> {
        let text = self.analysis.text(file)?;
        let mut indexes = self.indexes.borrow_mut();
        let index = indexes.entry(file).or_insert_with(|| LineIndex::new(text));
        Some(f(index, text))
    }

    /// The protocol range of `range` in `file`.
    #[must_use]
    pub fn range(&self, file: FileId, range: TextRange) -> Option<lsp::Range> {
        self.with_index(file, |index, text| index.range(text, range))
    }

    /// The protocol position of `offset` in `file`.
    #[must_use]
    pub fn position(&self, file: FileId, offset: TextSize) -> Option<lsp::Position> {
        self.with_index(file, |index, text| index.position(text, offset))
    }

    /// The byte offset of `position` in `file`, clamped.
    #[must_use]
    pub fn offset(&self, file: FileId, position: lsp::Position) -> Option<TextSize> {
        self.with_index(file, |index, text| index.offset(text, position))
    }

    /// The byte range of `range` in `file`, clamped.
    #[must_use]
    pub fn text_range(&self, file: FileId, range: lsp::Range) -> Option<TextRange> {
        self.with_index(file, |index, text| index.text_range(text, range))
    }

    /// The location of `span`.
    #[must_use]
    pub fn location(&self, span: Span) -> Option<lsp::Location> {
        Some(lsp::Location {
            uri: self.url(span.file)?,
            range: self.range(span.file, span.range)?,
        })
    }

    /// A link to `span` for Markdown: its file URL with a `#L<line>`
    /// fragment.
    #[must_use]
    pub fn link(&self, span: Span) -> Option<String> {
        let url = self.url(span.file)?;
        let line = self.position(span.file, span.range.start())?.line + 1;
        Some(format!("{url}#L{line}"))
    }
}

/// Locations of spans, dropping those that can't be placed.
fn locations(snap: &Snap, spans: Vec<Span>) -> Vec<lsp::Location> {
    spans.into_iter().filter_map(|s| snap.location(s)).collect()
}

/// `textDocument/hover`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn hover(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
) -> Result<Option<lsp::Hover>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let link = |span: Span| snap.link(span);
    let Some(result) = ide::hover::hover(&snap.analysis, file, offset, &link)? else {
        return Ok(None);
    };
    Ok(Some(lsp::Hover {
        contents: lsp::HoverContents::Markup(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: result.markdown,
        }),
        range: snap.range(file, result.range),
    }))
}

/// Which navigation request a [`goto`] answers.
#[derive(Clone, Copy, Debug)]
pub enum Goto {
    /// `textDocument/definition`.
    Definition,
    /// `textDocument/typeDefinition`.
    TypeDefinition,
    /// `textDocument/implementation`.
    Implementation,
}

/// The navigation requests.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn goto(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
    which: Goto,
) -> Result<Option<lsp::GotoDefinitionResponse>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let a = &snap.analysis;
    let spans = match which {
        Goto::Definition => ide::navigation::definition(a, file, offset)?,
        Goto::TypeDefinition => ide::navigation::type_definition(a, file, offset)?,
        Goto::Implementation => ide::navigation::implementation(a, file, offset)?,
    };
    Ok(Some(lsp::GotoDefinitionResponse::Array(locations(
        snap, spans,
    ))))
}

/// `textDocument/implementation` across a workspace
/// (`docs/design/40-workspaces-and-services.md`): the implementations in
/// the document's own project, and, for an interface or protocol (or a
/// member of one) declared in a library, those in every sibling member
/// that sees the same library file, so go to implementation on
/// `Payments.charge` in shop finds paybox's handler.
///
/// # Errors
///
/// [`Cancelled`] if a snapshot is stale.
pub fn implementation_across(
    snap: &Snap,
    siblings: &[Snap],
    file: FileId,
    position: lsp::Position,
) -> Result<Option<lsp::GotoDefinitionResponse>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let a = &snap.analysis;
    let mut out = locations(snap, ide::navigation::implementation(a, file, offset)?);
    let Some(target) = ide::navigation::target_at(a, file, offset)? else {
        return Ok(Some(lsp::GotoDefinitionResponse::Array(out)));
    };
    // Where the declaration is, on disk: the same file in every member.
    let view = a.program()?;
    let decl_url = snap.url(view.decl(target.decl).file);
    for other in siblings {
        let ov = other.analysis.program()?;
        let Some(d) = ov.lookup(&target.qualified) else {
            continue;
        };
        if other.url(ov.decl(d).file) != decl_url {
            continue;
        }
        let spans =
            ide::navigation::implementations_of(&other.analysis, d, target.member.as_ref())?;
        for l in locations(other, spans) {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    Ok(Some(lsp::GotoDefinitionResponse::Array(out)))
}

/// `textDocument/references`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn references(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
    include_declaration: bool,
) -> Result<Option<Vec<lsp::Location>>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let spans = ide::navigation::references(&snap.analysis, file, offset, include_declaration)?;
    Ok(Some(locations(snap, spans)))
}

/// `textDocument/documentHighlight`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn highlights(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
) -> Result<Option<Vec<lsp::DocumentHighlight>>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let found = ide::navigation::highlights(&snap.analysis, file, offset)?;
    Ok(Some(
        found
            .into_iter()
            .filter_map(|(range, access)| {
                Some(lsp::DocumentHighlight {
                    range: snap.range(file, range)?,
                    kind: Some(match access {
                        ide::navigation::Access::Write => lsp::DocumentHighlightKind::WRITE,
                        ide::navigation::Access::Read => lsp::DocumentHighlightKind::READ,
                    }),
                })
            })
            .collect(),
    ))
}

/// `textDocument/prepareRename`: the range and name, or why the symbol
/// can't be renamed.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn prepare_rename(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
) -> Result<Result<lsp::PrepareRenameResponse, String>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(Err("the position is outside the document".to_owned()));
    };
    Ok(match ide::rename::prepare(&snap.analysis, file, offset)? {
        Ok((range, name)) => match snap.range(file, range) {
            Some(range) => Ok(lsp::PrepareRenameResponse::RangeWithPlaceholder {
                range,
                placeholder: name,
            }),
            None => Err("the position is outside the document".to_owned()),
        },
        Err(reason) => Err(reason),
    })
}

/// `textDocument/rename`: the workspace edit, or why it can't be done.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn rename(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
    new_name: &str,
) -> Result<Result<lsp::WorkspaceEdit, String>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(Err("the position is outside the document".to_owned()));
    };
    let edits = match ide::rename::rename(&snap.analysis, file, offset, new_name)? {
        Ok(edits) => edits,
        Err(reason) => return Ok(Err(reason)),
    };
    let mut by_file: Vec<(FileId, Vec<lsp::TextEdit>)> = Vec::new();
    for (span, text) in edits.edits {
        let Some(range) = snap.range(span.file, span.range) else {
            continue;
        };
        let edit = lsp::TextEdit {
            range,
            new_text: text,
        };
        match by_file.iter_mut().find(|(f, _)| *f == span.file) {
            Some((_, list)) => list.push(edit),
            None => by_file.push((span.file, vec![edit])),
        }
    }
    if snap.caps.rename_files && !edits.file_renames.is_empty() {
        let mut ops = Vec::new();
        for (f, list) in by_file {
            let Some(uri) = snap.url(f) else { continue };
            ops.push(lsp::DocumentChangeOperation::Edit(lsp::TextDocumentEdit {
                text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                    uri,
                    version: snap.versions.get(&f).copied(),
                },
                edits: list.into_iter().map(lsp::OneOf::Left).collect(),
            }));
        }
        for (f, name) in edits.file_renames {
            let Some(old_uri) = snap.url(f) else { continue };
            let Some(path) = old_uri.to_file_path().ok() else {
                continue;
            };
            let Some(new_uri) = Url::from_file_path(path.with_file_name(&name)).ok() else {
                continue;
            };
            ops.push(lsp::DocumentChangeOperation::Op(lsp::ResourceOp::Rename(
                lsp::RenameFile {
                    old_uri,
                    new_uri,
                    options: None,
                    annotation_id: None,
                },
            )));
        }
        return Ok(Ok(lsp::WorkspaceEdit {
            document_changes: Some(lsp::DocumentChanges::Operations(ops)),
            ..lsp::WorkspaceEdit::default()
        }));
    }
    let mut changes = HashMap::new();
    for (f, list) in by_file {
        if let Some(uri) = snap.url(f) {
            changes.insert(uri, list);
        }
    }
    Ok(Ok(lsp::WorkspaceEdit {
        changes: Some(changes),
        ..lsp::WorkspaceEdit::default()
    }))
}

/// The protocol's kind of a symbol.
fn symbol_kind(kind: SymbolKind) -> lsp::SymbolKind {
    match kind {
        SymbolKind::Struct => lsp::SymbolKind::STRUCT,
        SymbolKind::Class => lsp::SymbolKind::CLASS,
        SymbolKind::Enum => lsp::SymbolKind::ENUM,
        SymbolKind::Interface => lsp::SymbolKind::INTERFACE,
        SymbolKind::Function => lsp::SymbolKind::FUNCTION,
        SymbolKind::Constant => lsp::SymbolKind::CONSTANT,
        SymbolKind::Field => lsp::SymbolKind::FIELD,
        SymbolKind::EnumMember => lsp::SymbolKind::ENUM_MEMBER,
        SymbolKind::Constructor => lsp::SymbolKind::CONSTRUCTOR,
        SymbolKind::Module => lsp::SymbolKind::MODULE,
        SymbolKind::Property => lsp::SymbolKind::PROPERTY,
    }
}

/// `textDocument/documentSymbol`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
#[allow(
    deprecated,
    reason = "`deprecated` is a required field of the protocol types"
)]
pub fn document_symbols(
    snap: &Snap,
    file: FileId,
) -> Result<Option<lsp::DocumentSymbolResponse>, Cancelled> {
    let found = ide::symbols::document_symbols(&snap.analysis, file)?;
    if !snap.caps.hierarchical_symbols {
        let Some(uri) = snap.url(file) else {
            return Ok(None);
        };
        let mut flat = Vec::new();
        for symbol in &found {
            let mut push = |s: &ide::symbols::DocumentSymbol, container: Option<String>| {
                if let Some(range) = snap.range(file, s.selection) {
                    flat.push(lsp::SymbolInformation {
                        name: s.name.clone(),
                        kind: symbol_kind(s.kind),
                        tags: None,
                        deprecated: None,
                        location: lsp::Location {
                            uri: uri.clone(),
                            range,
                        },
                        container_name: container,
                    });
                }
            };
            push(symbol, None);
            for child in &symbol.children {
                push(child, Some(symbol.name.clone()));
            }
        }
        return Ok(Some(lsp::DocumentSymbolResponse::Flat(flat)));
    }
    fn convert(
        snap: &Snap,
        file: FileId,
        s: ide::symbols::DocumentSymbol,
    ) -> Option<lsp::DocumentSymbol> {
        Some(lsp::DocumentSymbol {
            name: s.name,
            detail: s.detail,
            kind: symbol_kind(s.kind),
            tags: None,
            deprecated: None,
            range: snap.range(file, s.range)?,
            selection_range: snap.range(file, s.selection)?,
            children: Some(
                s.children
                    .into_iter()
                    .filter_map(|c| convert(snap, file, c))
                    .collect(),
            ),
        })
    }
    Ok(Some(lsp::DocumentSymbolResponse::Nested(
        found
            .into_iter()
            .filter_map(|s| convert(snap, file, s))
            .collect(),
    )))
}

/// `workspace/symbol`, over every project.
///
/// # Errors
///
/// [`Cancelled`] if a snapshot is stale.
#[allow(
    deprecated,
    reason = "`deprecated` is a required field of the protocol types"
)]
pub fn workspace_symbols(
    snaps: &[Snap],
    query: &str,
) -> Result<Vec<lsp::SymbolInformation>, Cancelled> {
    let mut out = Vec::new();
    for snap in snaps {
        for s in ide::symbols::workspace_symbols(&snap.analysis, query, 256)? {
            let Some(location) = snap.location(Span::new(s.file, s.range)) else {
                continue;
            };
            out.push(lsp::SymbolInformation {
                name: s.name,
                kind: symbol_kind(s.kind),
                tags: None,
                deprecated: None,
                location,
                container_name: s.container,
            });
        }
    }
    Ok(out)
}

/// `textDocument/completion`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn completion(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
) -> Result<Option<lsp::CompletionResponse>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let items = ide::completion::completions(&snap.analysis, file, offset)?;
    let items = items
        .into_iter()
        .map(|i| {
            let additional_text_edits = i.import.as_ref().and_then(|(at, text)| {
                let position = snap.position(file, *at)?;
                Some(vec![lsp::TextEdit {
                    range: lsp::Range::new(position, position),
                    new_text: text.clone(),
                }])
            });
            lsp::CompletionItem {
                label: i.label.clone(),
                kind: Some(item_kind(i.kind)),
                detail: i.detail,
                sort_text: Some(format!("{}{}", i.group, i.label)),
                additional_text_edits,
                ..lsp::CompletionItem::default()
            }
        })
        .collect();
    Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
        is_incomplete: false,
        items,
    })))
}

/// The protocol's kind of a completion item.
fn item_kind(kind: ItemKind) -> lsp::CompletionItemKind {
    match kind {
        ItemKind::Variable => lsp::CompletionItemKind::VARIABLE,
        ItemKind::Field => lsp::CompletionItemKind::FIELD,
        ItemKind::Method => lsp::CompletionItemKind::METHOD,
        ItemKind::Function => lsp::CompletionItemKind::FUNCTION,
        ItemKind::Constructor => lsp::CompletionItemKind::CONSTRUCTOR,
        ItemKind::EnumMember => lsp::CompletionItemKind::ENUM_MEMBER,
        ItemKind::Constant => lsp::CompletionItemKind::CONSTANT,
        ItemKind::Struct => lsp::CompletionItemKind::STRUCT,
        ItemKind::Class => lsp::CompletionItemKind::CLASS,
        ItemKind::Enum => lsp::CompletionItemKind::ENUM,
        ItemKind::Interface => lsp::CompletionItemKind::INTERFACE,
        ItemKind::Keyword => lsp::CompletionItemKind::KEYWORD,
        ItemKind::Annotation => lsp::CompletionItemKind::PROPERTY,
        ItemKind::Element => lsp::CompletionItemKind::SNIPPET,
    }
}

/// The UTF-16 offset of byte offset `at` in `label`.
fn utf16_offset(label: &str, at: usize) -> u32 {
    let at = at.min(label.len());
    u32::try_from(label.get(..at).map_or(0, |s| s.encode_utf16().count())).unwrap_or(u32::MAX)
}

/// `textDocument/signatureHelp`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn signature_help(
    snap: &Snap,
    file: FileId,
    position: lsp::Position,
) -> Result<Option<lsp::SignatureHelp>, Cancelled> {
    let Some(offset) = snap.offset(file, position) else {
        return Ok(None);
    };
    let Some(help) = ide::signature_help::signature_help(&snap.analysis, file, offset)? else {
        return Ok(None);
    };
    let signatures = help
        .signatures
        .iter()
        .map(|s| lsp::SignatureInformation {
            label: s.label.clone(),
            documentation: None,
            parameters: Some(
                s.params
                    .iter()
                    .map(|(start, end, _)| lsp::ParameterInformation {
                        label: lsp::ParameterLabel::LabelOffsets([
                            utf16_offset(&s.label, *start),
                            utf16_offset(&s.label, *end),
                        ]),
                        documentation: None,
                    })
                    .collect(),
            ),
            active_parameter: None,
        })
        .collect();
    Ok(Some(lsp::SignatureHelp {
        signatures,
        active_signature: u32::try_from(help.active_signature).ok(),
        active_parameter: u32::try_from(help.active_parameter).ok(),
    }))
}

/// `textDocument/codeAction`: the quick fixes of the diagnostics in the
/// range.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn code_actions(
    snap: &Snap,
    file: FileId,
    range: lsp::Range,
) -> Result<Option<Vec<lsp::CodeActionOrCommand>>, Cancelled> {
    let Some(range) = snap.text_range(file, range) else {
        return Ok(None);
    };
    let fixes = ide::fixes::quick_fixes(&snap.analysis, file, range)?;
    let mut out = Vec::new();
    for fix in fixes {
        let mut changes: HashMap<Url, Vec<lsp::TextEdit>> = HashMap::new();
        let mut complete = true;
        for edit in &fix.fix.edits {
            match (
                snap.url(edit.span.file),
                snap.range(edit.span.file, edit.span.range),
            ) {
                (Some(uri), Some(range)) => changes.entry(uri).or_default().push(lsp::TextEdit {
                    range,
                    new_text: edit.replacement.clone(),
                }),
                _ => complete = false,
            }
        }
        if !complete {
            continue;
        }
        out.push(lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
            title: fix.fix.message.clone(),
            kind: Some(lsp::CodeActionKind::QUICKFIX),
            diagnostics: diagnostic(snap, &fix.diagnostic).map(|d| vec![d]),
            edit: Some(lsp::WorkspaceEdit {
                changes: Some(changes),
                ..lsp::WorkspaceEdit::default()
            }),
            command: None,
            is_preferred: Some(fix.preferred),
            disabled: None,
            data: None,
        }));
    }
    Ok(Some(out))
}

/// `textDocument/inlayHint`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn inlay_hints(
    snap: &Snap,
    file: FileId,
    range: lsp::Range,
) -> Result<Option<Vec<lsp::InlayHint>>, Cancelled> {
    let Some(range) = snap.text_range(file, range) else {
        return Ok(None);
    };
    let hints = ide::inlay::inlay_hints(&snap.analysis, file, range)?;
    Ok(Some(
        hints
            .into_iter()
            .filter_map(|h| {
                Some(lsp::InlayHint {
                    position: snap.position(file, h.offset)?,
                    label: lsp::InlayHintLabel::String(h.label),
                    kind: None,
                    text_edits: None,
                    tooltip: Some(lsp::InlayHintTooltip::String(
                        "The page's URL, derived from its name and its path `@Param`s".to_owned(),
                    )),
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                })
            })
            .collect(),
    ))
}

/// Converts a compiler diagnostic: notes and helps become lines of the
/// message, labels become related information, unused-code warnings are
/// tagged as unnecessary.
#[must_use]
pub fn diagnostic(snap: &Snap, d: &KDiagnostic) -> Option<lsp::Diagnostic> {
    let range = snap.range(d.span.file, d.span.range)?;
    let mut message = d.message.clone();
    for note in &d.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    for help in &d.helps {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }
    let related: Vec<lsp::DiagnosticRelatedInformation> = d
        .labels
        .iter()
        .filter_map(|l| {
            Some(lsp::DiagnosticRelatedInformation {
                location: snap.location(l.span)?,
                message: l.message.clone(),
            })
        })
        .collect();
    let name = d.code.info().map_or("", |i| i.name);
    let tags = name
        .starts_with("unused-")
        .then(|| vec![lsp::DiagnosticTag::UNNECESSARY]);
    Some(lsp::Diagnostic {
        range,
        severity: Some(match d.severity {
            Severity::Error => lsp::DiagnosticSeverity::ERROR,
            Severity::Warning => lsp::DiagnosticSeverity::WARNING,
        }),
        code: Some(lsp::NumberOrString::String(d.code.to_string())),
        code_description: None,
        source: Some("k".to_owned()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        tags,
        data: None,
    })
}

/// Every diagnostic of the project, by file URL: for every project file
/// that has some, and (possibly empty) for every open document.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn project_diagnostics(snap: &Snap) -> Result<Vec<(Url, Vec<lsp::Diagnostic>)>, Cancelled> {
    let all = snap.analysis.diagnostics()?;
    let mut by_file: Vec<(FileId, Vec<lsp::Diagnostic>)> = Vec::new();
    for d in &all {
        if ide::is_stdlib(&snap.analysis, d.span.file) {
            continue;
        }
        // A mounted library file's diagnostics are the library project's
        // to publish (its own analysis reports them).
        if let Some(k_sema::FileOrigin::Project(path)) = snap.analysis.origin(d.span.file) {
            if path.dependency().is_some() {
                continue;
            }
        }
        let Some(converted) = diagnostic(snap, d) else {
            continue;
        };
        match by_file.last_mut() {
            Some((f, list)) if *f == d.span.file => list.push(converted),
            _ => by_file.push((d.span.file, vec![converted])),
        }
    }
    // Open documents without diagnostics get an empty list, so an editor
    // knows they have been checked.
    let mut open: Vec<FileId> = snap.versions.keys().copied().collect();
    open.sort_unstable();
    for f in open {
        if !by_file.iter().any(|(g, _)| *g == f) {
            by_file.push((f, Vec::new()));
        }
    }
    Ok(by_file
        .into_iter()
        .filter_map(|(f, list)| Some((snap.url(f)?, list)))
        .collect())
}

//! Warm re-checks: replaying the body facts of unchanged files (sema.md
//! §3.3, "Round 2 notes").
//!
//! The semantic pass runs once per revision over the whole project, but
//! most of its time goes to checking bodies, and a body's check depends only
//! on its own file's text and on the project's *signatures*. So each file's
//! body check is recorded as [`FileFacts`]: the diagnostics, types,
//! resolutions and side facts (used imports, used classes, …) it produced.
//! The next revision replays the facts of every file whose text didn't
//! change, instead of checking it again, as long as no signature in the
//! project changed.
//!
//! # What a signature is
//!
//! A file's signature text ([`signature_text`]) is its token stream without
//! trivia and without the bodies of functions, methods, initializers and
//! rpcs. Everything else (field types and initializers, views, styles,
//! annotations) counts as signature, conservatively. Positions don't count:
//! a body edit that shifts the declarations after it keeps the signature,
//! and the facts that mention another file's positions record that file as
//! a dependency instead ([`FileFacts::deps`]).
//!
//! # Why replay is exact
//!
//! * The declarations, members and generics of two revisions with the same
//!   file list and the same signatures get the same ids, so the facts' ids
//!   stay valid.
//! * Every signature is resolved eagerly before the bodies
//!   (`check::resolve_signatures`), so the facts a lazy signature
//!   resolution produces (its diagnostics, its type-name resolutions, the
//!   imports it uses) never land in a body's facts: they are produced
//!   again, identically, in every revision.
//! * The project-level passes (routing, DI, unused imports, …) run after the
//!   bodies in every revision, over the replayed facts.
//!
//! The incremental property tests (`soracle::check_incremental`) and the
//! `check_typed` fuzz target compare every incremental re-check with a fresh
//! one.

use std::sync::Arc;

use k_diagnostics::{Diagnostic, FileId, TextRange};
use k_syntax::{SyntaxKind, SyntaxNode};
use rowan::WalkEvent;

use crate::fx::FxHashMap;
use crate::lookup::Resolution;
use crate::program::{DeclId, FileIx, MemberId};
use crate::ty::Ty;

/// Facts keyed by a file and a range: types or resolutions.
pub(crate) type RangeMap<V> = FxHashMap<(FileId, TextRange), V>;

/// A [`RangeMap`] split into shared chunks: one revision's live chunk (the
/// facts of signatures and project passes) and each file's body chunk,
/// shared with the cache so a replay copies nothing.
///
/// A key is in at most one chunk in practice; [`Chunked::get`] returns the
/// first match. Lookups go through a per-file index of the chunks that hold
/// keys of that file, built on the first lookup of the revision (O(entries)
/// once), so a lookup costs a hash probe for the file plus one per chunk
/// that mentions it, usually one or two. Without the index a lookup probed
/// every chunk, which made code generation quadratic in the number of files
/// (`docs/compiler/performance.md` §11.4). The check itself never looks
/// facts up through this type, so building the index lazily keeps it off
/// the warm re-check path.
#[derive(Debug)]
pub(crate) struct Chunked<V> {
    chunks: Vec<Arc<RangeMap<V>>>,
    /// For each file, the indices (ascending) of the chunks with a key in
    /// it.
    by_file: std::sync::OnceLock<FxHashMap<FileId, Vec<u32>>>,
}

impl<V> Default for Chunked<V> {
    fn default() -> Chunked<V> {
        Chunked {
            chunks: Vec::new(),
            by_file: std::sync::OnceLock::new(),
        }
    }
}

impl<V> Chunked<V> {
    /// Adds a chunk; empty chunks are dropped. Must not be called after the
    /// first [`get`](Chunked::get) (the result of a revision is immutable
    /// once built; the index would be stale).
    pub(crate) fn push(&mut self, chunk: Arc<RangeMap<V>>) {
        debug_assert!(self.by_file.get().is_none(), "push after get");
        if !chunk.is_empty() {
            self.chunks.push(chunk);
        }
    }

    /// The value at `key`, if any chunk has one.
    pub(crate) fn get(&self, key: &(FileId, TextRange)) -> Option<&V> {
        let index = self.by_file.get_or_init(|| self.index());
        index
            .get(&key.0)?
            .iter()
            .find_map(|&i| self.chunks.get(i as usize)?.get(key))
    }

    /// Builds the per-file chunk index.
    fn index(&self) -> FxHashMap<FileId, Vec<u32>> {
        let mut by_file: FxHashMap<FileId, Vec<u32>> = FxHashMap::default();
        for (i, chunk) in self.chunks.iter().enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            // A body chunk's keys are almost all in one file: skip the
            // probe while the file repeats.
            let mut last = None;
            for (file, _) in chunk.keys() {
                if last == Some(*file) {
                    continue;
                }
                last = Some(*file);
                let list = by_file.entry(*file).or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
        }
        by_file
    }
}

/// What checking one file's bodies produced: everything the body phase of
/// `check::analyze` leaves behind for the project passes and the queries.
#[derive(Debug)]
pub(crate) struct FileFacts {
    /// The file's text when it was checked; a replay requires the same
    /// text (compared by pointer: the host keeps a file's `Arc` while its
    /// text is unchanged, and this one keeps it alive).
    pub(crate) text: Arc<str>,
    /// The file's signature text ([`signature_text`]).
    pub(crate) signature: Arc<str>,
    /// Other files whose positions the facts mention (a label or an edit
    /// in another file): a replay also requires their texts unchanged.
    pub(crate) deps: Vec<FileId>,
    /// The diagnostics.
    pub(crate) diags: Vec<Diagnostic>,
    /// The recorded types.
    pub(crate) types: Arc<RangeMap<Ty>>,
    /// The recorded resolutions.
    pub(crate) resolutions: Arc<RangeMap<Resolution>>,
    /// Imports used by name lookups, as `(file, import index)`.
    pub(crate) used_imports: Vec<(FileIx, usize)>,
    /// Style classes used by views, by declaration and name.
    pub(crate) used_classes: Vec<(DeclId, String)>,
    /// Keyframes used by style blocks, by declaration and name.
    pub(crate) used_keyframes: Vec<(DeclId, String)>,
    /// `@Stylable` fields applied with `.class(…)`.
    pub(crate) applied_stylables: Vec<MemberId>,
    /// What each `@Load` initializer throws, and its range.
    pub(crate) loads: Vec<(MemberId, (Option<Ty>, TextRange))>,
}

/// The body facts of one revision, handed to the next one
/// ([`crate::host`] keeps it with the revision's result).
#[derive(Debug, Default)]
pub(crate) struct BodyCache {
    /// The project files in input order, with their paths: ids are only
    /// stable while this list is the same.
    pub(crate) file_list: Vec<(FileId, String)>,
    /// The facts of each project file.
    pub(crate) files: FxHashMap<FileId, Arc<FileFacts>>,
}

/// Returns the signature text of a file: every non-trivia token, each as
/// its length, `:` and its text (so concatenations can't collide), except
/// those inside the body block of a function, method, initializer or rpc.
///
/// One preorder walk of the tree; bodies are skipped, not visited.
pub(crate) fn signature_text(root: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut walk = root.preorder_with_tokens();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(element) = event else {
            continue;
        };
        match element {
            rowan::NodeOrToken::Node(node) => {
                let is_body = node.kind() == SyntaxKind::Block
                    && node.parent().is_some_and(|p| {
                        matches!(
                            p.kind(),
                            SyntaxKind::FuncDecl | SyntaxKind::InitDecl | SyntaxKind::RpcDecl
                        )
                    });
                if is_body {
                    out.push_str("{}");
                    walk.skip_subtree();
                }
            }
            rowan::NodeOrToken::Token(token) => {
                if !token.kind().is_trivia() {
                    let text = token.text();
                    out.push_str(&text.len().to_string());
                    out.push(':');
                    out.push_str(text);
                }
            }
        }
    }
    out
}

/// The files other than `own` whose positions `diags` mention: label
/// spans and fix edits.
pub(crate) fn foreign_files(own: FileId, diags: &[Diagnostic]) -> Vec<FileId> {
    let mut out: Vec<FileId> = Vec::new();
    for d in diags {
        let spans = std::iter::once(d.span)
            .chain(d.labels.iter().map(|l| l.span))
            .chain(d.fixes.iter().flat_map(|f| f.edits.iter().map(|e| e.span)));
        for span in spans {
            if span.file != own && !out.contains(&span.file) {
                out.push(span.file);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(text: &str) -> String {
        let parse = k_syntax::parse(FileId::new(0), text);
        signature_text(&parse.syntax())
    }

    #[test]
    fn bodies_and_trivia_are_not_signature() {
        let a = sig("func f() -> Int {\n    return 1\n}\n");
        let b = sig("// changed\nfunc f() -> Int {\n    let x = 2\n    return x\n}\n");
        assert_eq!(a, b);
    }

    #[test]
    fn signatures_and_field_initializers_are() {
        let base = sig("struct S {\n    let a: Int = 1\n}\n");
        assert_ne!(base, sig("struct S {\n    let a: Int = 2\n}\n"));
        assert_ne!(base, sig("struct S {\n    let a: Double = 1\n}\n"));
        assert_ne!(
            sig("func f() -> Int {\n}\n"),
            sig("func f() -> Double {\n}\n")
        );
    }

    #[test]
    fn token_boundaries_are_kept() {
        // `ab` and `a b` differ although their concatenations agree.
        assert_ne!(sig("let ab = 1\n"), sig("let a b = 1\n"));
    }
}

//! A simple, append-only store of source files for rendering diagnostics.

use crate::line_index::{LineCol, LineIndex};
use crate::span::FileId;
use text_size::TextSize;

/// One source file: its display path, its text and its line index.
#[derive(Clone, Debug)]
pub struct SourceFile {
    /// The path shown in rendered locations (`path:line:col`); not
    /// necessarily a path that exists on disk.
    path: String,
    /// The complete source text, exactly as added.
    text: String,
    /// Line starts of `text`, computed once when the file is added.
    line_index: LineIndex,
}

impl SourceFile {
    /// Returns the path used when displaying locations in this file.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the full text of the file.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the line index of the file.
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// Converts a byte offset in this file into a [`LineCol`].
    #[must_use]
    pub fn line_col(&self, offset: TextSize) -> LineCol {
        self.line_index.line_col(&self.text, offset)
    }
}

/// An append-only collection of [`SourceFile`]s, indexed by [`FileId`].
///
/// The command-line driver and the test harness use a `SourceMap` to hand
/// out file ids and to give renderers access to source text. The incremental
/// query database (see `docs/compiler/architecture.md`) will provide the same
/// information through queries.
///
/// A `SourceMap` is plain owned data: it is `Send` and `Sync`, and it can be
/// shared across threads behind a reference once all files are added.
///
/// # Example
///
/// ```
/// use k_diagnostics::{LineCol, SourceMap, TextSize};
///
/// let mut sources = SourceMap::new();
/// let a = sources.add("src/A.k", "func f() {}\n");
/// let b = sources.add("src/B.k", "x\ny\n");
/// assert_eq!((a.raw(), b.raw()), (0, 1));
///
/// let file = sources.get(b).expect("issued by this map");
/// assert_eq!(file.path(), "src/B.k");
/// assert_eq!(file.line_col(TextSize::from(2)), LineCol { line: 2, col: 1 });
/// ```
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    /// The files, where the file with id `n` is at index `n`.
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty source map.
    #[must_use]
    pub fn new() -> SourceMap {
        SourceMap::default()
    }

    /// Adds a file and returns its id. Ids are assigned densely from zero in
    /// insertion order.
    ///
    /// Building the line index is linear in the length of `text`. Adding the
    /// same path twice adds a second, independent file. After `u32::MAX`
    /// files every further id saturates to `u32::MAX` (unreachable in
    /// practice).
    pub fn add(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let text = text.into();
        let line_index = LineIndex::new(&text);
        let id = FileId::new(u32::try_from(self.files.len()).unwrap_or(u32::MAX));
        self.files.push(SourceFile {
            path: path.into(),
            text,
            line_index,
        });
        id
    }

    /// Returns the file with the given id, or `None` if this map did not issue
    /// that id.
    #[must_use]
    pub fn get(&self, file: FileId) -> Option<&SourceFile> {
        self.files.get(file.raw() as usize)
    }

    /// Returns the number of files in the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns `true` if the map holds no files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

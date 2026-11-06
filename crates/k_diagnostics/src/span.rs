//! File identities and source spans.

use std::fmt;

use text_size::{TextRange, TextSize};

/// Identifies one source file within a compilation.
///
/// A `FileId` is an opaque, dense index handed out by a
/// [`SourceMap`](crate::SourceMap) (or, later, by the incremental query
/// database). It is cheap to copy and compare. A `FileId` is only meaningful
/// together with the map that issued it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    /// Creates a file id from its raw index.
    ///
    /// Most code obtains ids from [`SourceMap::add`](crate::SourceMap::add)
    /// instead. This constructor exists for tests and for stages that receive
    /// ids from elsewhere.
    #[must_use]
    pub const fn new(raw: u32) -> FileId {
        FileId(raw)
    }

    /// Returns the raw index of this file id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

/// A range of source text within a specific file.
///
/// The range is a half-open interval of UTF-8 byte offsets,
/// `start..end`. An empty range (`start == end`) denotes an insertion point,
/// for example "a `}` is missing here".
///
/// Spans are ordered by file, then start offset, then end offset, which is
/// the order in which diagnostics are reported.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Span {
    /// The file the range belongs to.
    pub file: FileId,
    /// The byte range within the file.
    pub range: TextRange,
}

impl Ord for Span {
    fn cmp(&self, other: &Span) -> std::cmp::Ordering {
        (self.file, self.range.start(), self.range.end()).cmp(&(
            other.file,
            other.range.start(),
            other.range.end(),
        ))
    }
}

impl PartialOrd for Span {
    fn partial_cmp(&self, other: &Span) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Span {
    /// Creates a span covering `range` in `file`.
    #[must_use]
    pub const fn new(file: FileId, range: TextRange) -> Span {
        Span { file, range }
    }

    /// Creates an empty span (an insertion point) at `offset` in `file`.
    #[must_use]
    pub fn empty(file: FileId, offset: TextSize) -> Span {
        Span {
            file,
            range: TextRange::empty(offset),
        }
    }

    /// Returns the byte offset where the span starts.
    #[must_use]
    pub fn start(self) -> TextSize {
        self.range.start()
    }

    /// Returns the byte offset just past the end of the span.
    #[must_use]
    pub fn end(self) -> TextSize {
        self.range.end()
    }
}

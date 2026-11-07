//! The project-wide reverse index of resolutions: for every symbol, the
//! spans that refer to it, and for every file, its references sorted by
//! position (`docs/compiler/sema.md` §17, "Tooling queries").
//!
//! # Incrementality
//!
//! A revision's resolutions are a [`Chunked`] map: one live chunk (the
//! facts of signatures and the project passes) plus one body chunk per
//! file, shared by pointer with the previous revision whenever the file's
//! body facts were replayed (`crate::warm`). The index mirrors that
//! layout: it is a list of per-chunk indexes ([`ChunkRefs`]), and a new
//! revision reuses the index of every chunk it shares with the previous
//! one, matched by pointer. After a body edit only the edited file's
//! chunk and the live chunk are indexed again; the rest is reused as is.
//!
//! The index is built on the first tooling query of a revision, never by
//! the check itself, so it costs `k check` nothing.

use std::sync::Arc;

use k_diagnostics::{FileId, Span, TextRange};

use crate::fx::FxHashMap;
use crate::lookup::Resolution;
use crate::warm::{Chunked, RangeMap};

/// The reverse index of one chunk of resolutions.
#[derive(Debug, Default)]
pub(crate) struct ChunkRefs {
    /// The spans that refer to each symbol, sorted.
    by_target: FxHashMap<Arc<Resolution>, Vec<Span>>,
    /// Each file's references, sorted by range start, then end.
    by_file: FxHashMap<FileId, Vec<(TextRange, Arc<Resolution>)>>,
}

impl ChunkRefs {
    /// Indexes one chunk. O(n log n) in its entries.
    fn build(chunk: &RangeMap<Resolution>) -> ChunkRefs {
        let mut refs = ChunkRefs::default();
        // One shared `Arc` per distinct resolution: the two maps point at
        // the same allocation.
        let mut interned: FxHashMap<&Resolution, Arc<Resolution>> = FxHashMap::default();
        for ((file, range), resolution) in chunk {
            let shared = Arc::clone(
                interned
                    .entry(resolution)
                    .or_insert_with(|| Arc::new(resolution.clone())),
            );
            refs.by_target
                .entry(Arc::clone(&shared))
                .or_default()
                .push(Span::new(*file, *range));
            refs.by_file
                .entry(*file)
                .or_default()
                .push((*range, shared));
        }
        for spans in refs.by_target.values_mut() {
            spans.sort_unstable();
        }
        for list in refs.by_file.values_mut() {
            list.sort_unstable_by_key(|(r, _)| (r.start(), r.end()));
        }
        refs
    }
}

/// The reverse index of one revision: one [`ChunkRefs`] per chunk of its
/// resolutions, in chunk order, each paired with the chunk it indexes so a
/// later revision can recognize the chunks it shares.
#[derive(Debug, Default)]
pub(crate) struct RefIndex {
    /// Each resolution chunk with its reverse index, in chunk order.
    chunks: Vec<(Arc<RangeMap<Resolution>>, Arc<ChunkRefs>)>,
}

impl RefIndex {
    /// Indexes `resolutions`, reusing the per-chunk indexes of `prev` for
    /// every chunk it shares with it (compared by pointer).
    pub(crate) fn build(resolutions: &Chunked<Resolution>, prev: Option<&RefIndex>) -> RefIndex {
        let reusable: FxHashMap<*const RangeMap<Resolution>, &Arc<ChunkRefs>> = prev
            .map(|p| {
                p.chunks
                    .iter()
                    .map(|(chunk, refs)| (Arc::as_ptr(chunk), refs))
                    .collect()
            })
            .unwrap_or_default();
        let chunks = resolutions
            .chunks()
            .iter()
            .map(|chunk| {
                let refs = reusable.get(&Arc::as_ptr(chunk)).map_or_else(
                    || Arc::new(ChunkRefs::build(chunk)),
                    |refs| Arc::clone(refs),
                );
                (Arc::clone(chunk), refs)
            })
            .collect();
        RefIndex { chunks }
    }

    /// Every span that refers to `target`, sorted and without duplicates.
    pub(crate) fn references(&self, target: &Resolution) -> Vec<Span> {
        let mut out: Vec<Span> = self
            .chunks
            .iter()
            .filter_map(|(_, refs)| refs.by_target.get(target))
            .flatten()
            .copied()
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The references of `file`, sorted by range. Where two chunks hold the
    /// same range (they shouldn't), the earlier chunk's resolution wins, as
    /// in [`Chunked::get`].
    pub(crate) fn file_references(&self, file: FileId) -> Vec<(TextRange, Arc<Resolution>)> {
        let mut out: Vec<(TextRange, Arc<Resolution>)> = Vec::new();
        for (_, refs) in &self.chunks {
            if let Some(list) = refs.by_file.get(&file) {
                out.extend(list.iter().cloned());
            }
        }
        // A stable sort keeps the earlier chunk first among equal ranges.
        out.sort_by_key(|(r, _)| (r.start(), r.end()));
        out.dedup_by_key(|(r, _)| *r);
        out
    }

    /// The number of chunks whose index this revision built itself rather
    /// than reused (for tests of the incremental behaviour).
    #[cfg(test)]
    pub(crate) fn rebuilt_since(&self, prev: &RefIndex) -> usize {
        self.chunks
            .iter()
            .filter(|(_, refs)| !prev.chunks.iter().any(|(_, old)| Arc::ptr_eq(old, refs)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(file: u32, start: u32, end: u32) -> (FileId, TextRange) {
        (FileId::new(file), TextRange::new(start.into(), end.into()))
    }

    fn decl(name: &str) -> Resolution {
        Resolution::Declaration {
            name: name.to_owned(),
        }
    }

    #[test]
    fn chunks_index_targets_and_files() {
        let mut a: RangeMap<Resolution> = RangeMap::default();
        a.insert(span(1, 10, 12), decl("m.A"));
        a.insert(span(1, 2, 4), decl("m.A"));
        a.insert(span(2, 0, 1), decl("m.B"));
        let mut b: RangeMap<Resolution> = RangeMap::default();
        b.insert(span(3, 5, 6), decl("m.A"));
        let mut chunked = Chunked::default();
        chunked.push(Arc::new(a));
        chunked.push(Arc::new(b));
        let index = RefIndex::build(&chunked, None);
        let refs = index.references(&decl("m.A"));
        assert_eq!(
            refs,
            vec![
                Span::new(FileId::new(1), TextRange::new(2.into(), 4.into())),
                Span::new(FileId::new(1), TextRange::new(10.into(), 12.into())),
                Span::new(FileId::new(3), TextRange::new(5.into(), 6.into())),
            ]
        );
        assert!(index.references(&decl("m.C")).is_empty());
        let file1 = index.file_references(FileId::new(1));
        let ranges: Vec<TextRange> = file1.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            ranges,
            vec![
                TextRange::new(2.into(), 4.into()),
                TextRange::new(10.into(), 12.into())
            ]
        );
        assert!(index.file_references(FileId::new(9)).is_empty());
    }

    #[test]
    fn shared_chunks_reuse_their_index() {
        let shared: Arc<RangeMap<Resolution>> = Arc::new(
            [(span(1, 0, 1), decl("m.A"))]
                .into_iter()
                .collect::<RangeMap<Resolution>>(),
        );
        let mut first = Chunked::default();
        first.push(Arc::new(
            [(span(2, 0, 1), decl("m.B"))]
                .into_iter()
                .collect::<RangeMap<Resolution>>(),
        ));
        first.push(Arc::clone(&shared));
        let before = RefIndex::build(&first, None);
        let mut second = Chunked::default();
        second.push(Arc::new(
            [(span(2, 0, 1), decl("m.C"))]
                .into_iter()
                .collect::<RangeMap<Resolution>>(),
        ));
        second.push(Arc::clone(&shared));
        let after = RefIndex::build(&second, Some(&before));
        assert_eq!(after.rebuilt_since(&before), 1, "only the new chunk");
        assert_eq!(after.references(&decl("m.A")).len(), 1);
        assert_eq!(after.references(&decl("m.C")).len(), 1);
        assert!(after.references(&decl("m.B")).is_empty());
    }
}

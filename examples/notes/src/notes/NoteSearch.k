import k.concurrency.Worker
import k.concurrency.WorkerOnlyError
import k.crypto.Uuid

/// Ranks the account's notes for a search, in a Web Worker
/// (`docs/design/55-workers.md`).
///
/// Scoring every note on every key press is the one piece of the notes list
/// whose cost grows with the account: with thousands of notes it would
/// make typing into the filter stutter if it ran on the main thread. As a
/// `@Worker` service it runs off the page's thread; the page passes the
/// notes and the query, and gets the ids back, best first. It keeps
/// nothing between calls, so the `Notes` page cancelling a stale search
/// (by typing on) simply stops it.
@Worker
public service NoteSearch {
    /// The ids of the notes containing every word of `query` in their
    /// title, text or tags, best first: a word in the title counts 3, in a
    /// tag 2 and in the text 1. Ties keep the order of `notes`.
    ///
    /// - Parameters:
    ///   - notes: the notes to search, in the order to keep for ties.
    ///   - query: the text as typed; letter case is ignored.
    /// - Returns: the matching notes' ids; every note's for a blank query.
    func rank(notes: List<NoteSummary>, query: String) async throws WorkerOnlyError -> List<Uuid> {
        let words = query.lowercased().split(separator: " ").filter { word => word.isNotEmpty }
        let scored = notes.compactMap { note => scoredHit(note, words) }
        return scored.sortedByDescending { hit => hit.points }.map { hit => hit.id }
    }

    /// `note` with its score for `words`, or `nil` when a word is missing.
    private func scoredHit(_ note: NoteSummary, _ words: List<String>) -> SearchHit? {
        let title = note.title.lowercased()
        let body = note.body.lowercased()
        let tags = note.tags.map { tag => tag.lowercased() }
        var points = 0
        for word in words {
            let inTitle = title.contains(word)
            let inTag = tags.any { tag => tag.contains(word) }
            let inBody = body.contains(word)
            guard inTitle or inTag or inBody else {
                return nil
            }
            if inTitle {
                points += 3
            }
            if inTag {
                points += 2
            }
            if inBody {
                points += 1
            }
        }
        return SearchHit(id: note.id, points: points)
    }
}

/// A note that matched a search, with its score.
struct SearchHit {
    /// The note's id.
    let id: Uuid

    /// How well it matched: higher is better.
    let points: Int
}

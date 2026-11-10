import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections
import k.time.Clock

/// Edits that couldn't be saved because the device was offline, kept in
/// this browser's IndexedDB (`docs/design/52-indexed-collections.md`) so
/// that closing the tab, or the browser, doesn't lose them.
///
/// One edit per note, keyed by the note's id: a newer failed save replaces
/// the older one. The `savedAt` index lists the newest first on the notes
/// page. Injecting `Collections` makes this service client-only, so it runs
/// only in the browser; the components that show its edits are
/// `@Rendering(.client)`.
public service OfflineEdits {
    @Inject let collections: Collections
    @Inject let clock: Clock

    // The collection's name, key and value shape are recorded in
    // storage.lock, and the browser's database is upgraded to its indexes.
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")

    /// The newest unsaved edits, newest first, as `loadNewest()` last read
    /// them.
    @State var newest: List<UnsavedEdit> = []

    /// Keeps `draft` as the unsaved edit of `note`.
    ///
    /// - Returns: `false` when this browser couldn't keep it either (its
    ///   storage is blocked or full), so the page can say the text exists
    ///   only on screen.
    func keep(note: Uuid, draft: NoteDraft) async -> Bool {
        let edit = UnsavedEdit(noteId: note, title: draft.title, body: draft.body, tags: draft.tags, savedAt: clock.now)

        do {
            try await collections.put(edit, into: edits)
            return true
        } catch .unavailable, .quotaExceeded {
            return false
        }
    }

    /// The unsaved edit of `note`, or `nil`. An edit this version of the
    /// app can't read counts as none.
    func edit(of note: Uuid) async -> UnsavedEdit? {
        do {
            return try await collections.get(note, from: edits)
        } catch .unavailable, .unreadable {
            return nil
        }
    }

    /// Forgets the unsaved edit of `note`, once it is saved or discarded.
    func discard(note: Uuid) async {
        do {
            try await collections.delete(note, from: edits)
        } catch .unavailable, .quotaExceeded {
            // Nothing to keep: the edit stays, and is offered again.
        }
        newest = newest.filter { e => e.noteId != note }
    }

    /// Reads the five newest unsaved edits into `newest`.
    func loadNewest() async {
        do {
            newest = try await collections.range(in: bySavedAt, reversed: true, limit: 5)
        } catch .unavailable, .unreadable {
            newest = []
        }
    }
}

// A collection keyed by a `Uuid`, with indexes on an optional `Uuid`, a
// `Date`, an `Int`, a payload-free enum and a `String`, as a field and as
// top-level constants: no diagnostics (52-indexed-collections.md).
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections
import k.storage.StorageReadError

let byTitle: CollectionIndex<String, Draft> = CollectionIndex(collection: "drafts", field: "title")
let byMood: CollectionIndex<Mood, Draft> = CollectionIndex(collection: "drafts", field: "mood")

/// Drafts in IndexedDB.
public service Drafts {
    @Inject let collections: Collections

    private let drafts: Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")
    private let byNote: CollectionIndex<Uuid, Draft> = CollectionIndex(collection: "drafts", field: "noteId")
    private let bySavedAt: CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")
    private let byWords: CollectionIndex<Int, Draft> = CollectionIndex(collection: "drafts", field: "words")

    func of(note: Uuid) async throws StorageReadError -> List<Draft> {
        return try await collections.find(note, in: byNote)
    }

    func newest() async throws StorageReadError -> List<Draft> {
        return try await collections.range(in: bySavedAt, reversed: true, limit: 3)
    }

    func long() async throws StorageReadError -> List<Draft> {
        return try await collections.range(in: byWords, from: 1000)
    }
}

// An optional field added and a new index: compatible with storage.lock.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

/// Edits kept in the browser.
public service OfflineEdits {
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")
}

let byTitle: CollectionIndex<String, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "title")

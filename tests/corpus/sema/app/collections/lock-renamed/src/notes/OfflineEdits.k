// Renaming the collection leaves users' values under the old name: K8009,
// found by the recorded declaration.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

/// Edits kept in the browser.
public service OfflineEdits {
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "edits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "edits", field: "savedAt")
}

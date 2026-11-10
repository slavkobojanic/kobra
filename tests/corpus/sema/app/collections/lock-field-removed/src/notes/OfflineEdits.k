// Removing a recorded field, and adding a required one, would make stored
// values unreadable: K8009 at the collection, one note per change.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

/// Edits kept in the browser.
public service OfflineEdits {
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")
}

// With `@Breaking`, a new shape is allowed.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

/// Edits kept in the browser.
public service OfflineEdits {
    @Breaking("drafts are only a convenience")
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")
}

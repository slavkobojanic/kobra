// A different key field is K8009 even with `@Breaking`: stored values keep
// the key they were written with.
import k.crypto.Uuid
import k.storage.Collection

/// Edits kept in the browser.
public service OfflineEdits {
    @Breaking("rekeyed")
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "editId")
}

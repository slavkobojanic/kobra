// Two collections named `drafts` would share one IndexedDB store, and two
// indexes on one field one index: K2410 at the later literal, with a label
// at the earlier one. Test code may redeclare both.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

let drafts: Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")
let bySavedAt: CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")

/// Another service with the same names.
service Archive {
    private let archived: Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")
    private let byTime: CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")
}

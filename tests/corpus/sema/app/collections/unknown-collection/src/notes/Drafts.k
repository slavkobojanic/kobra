// An index names a declared collection of the same value type: K2412
// otherwise, with a label at the collection when it holds another type.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

let drafts: Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")
let typo: CollectionIndex<Date, Draft> = CollectionIndex(collection: "draft", field: "savedAt")
let otherType: CollectionIndex<Date, Other> = CollectionIndex(collection: "drafts", field: "savedAt")

import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

let stored: Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")
let storedByTime: CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")

test Drafts {
    test "a test may name an existing collection" {
        expect stored.name == "drafts"
    }
}

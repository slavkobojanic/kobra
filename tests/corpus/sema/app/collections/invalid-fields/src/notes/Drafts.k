// A key or indexed field must be a stored field of the value type, a
// struct, with the declared type and a key type (52-indexed-collections.md,
// "Declarations"); the field's name is a string literal, and a collection
// is declared as a constant or a field: K2411 each.
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex

let keyName: String = "id"

let missing: Collection<Uuid, Draft> = Collection(name: "a", key: "uuid")
let optionalKey: Collection<String, Tagged> = Collection(name: "b", key: "label")
let wrongType: Collection<Int, Draft> = Collection(name: "c", key: "id")
let structKey: Collection<NoteId, Tagged> = Collection(name: "d", key: "note")
let computed: Collection<Uuid, Draft> = Collection(name: "e", key: keyName)
let notStruct: Collection<String, String> = Collection(name: "f", key: "count")
let tagged: Collection<Uuid, Tagged> = Collection(name: "tagged", key: "id")
let byDone: CollectionIndex<Bool, Tagged> = CollectionIndex(collection: "tagged", field: "done")
let byTags: CollectionIndex<List<String>, Tagged> = CollectionIndex(collection: "tagged", field: "tags")
let byLabel: CollectionIndex<Int, Tagged> = CollectionIndex(collection: "tagged", field: "label")

/// Makes a collection inline.
func inline() -> String {
    let local: Collection<Uuid, Draft> = Collection(name: "g", key: "id")
    return local.name
}

let storeName: String = "h"
let computedName: Collection<Uuid, Draft> = Collection(name: storeName, key: "id")

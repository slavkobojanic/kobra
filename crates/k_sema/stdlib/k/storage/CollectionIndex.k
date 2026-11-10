/// A field of a [`Collection`]'s values to look them up and order them by
/// (`docs/design/52-indexed-collections.md`):
/// `CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")`.
/// Both arguments are string literals: `collection` names a declared
/// collection of `Value`s, and `field` a stored field of `Value` of type
/// `Field` or `Field?`, which is also the index's name. A value whose field
/// is `nil` isn't in the index. Declared as a constant or a field, and
/// queried through [`Collections.find(_:in:)`] and
/// [`Collections.range(in:from:to:reversed:limit:)`].
public struct CollectionIndex<Field, Value> where Field implements Codable, Value implements Codable {
    /// The name of the collection.
    let collection: String
    /// The indexed field's name, which is also the index's.
    let field: String
}

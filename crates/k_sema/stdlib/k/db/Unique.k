/// Adds a unique index on an entity field; the repository's `insert` and
/// `update` then throw the entity's generated `UniqueViolation`, with one
/// case per unique field. In the prelude.
@Annotation
public struct Unique {}

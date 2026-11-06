/// A unique index on an entity field; `insert` and `update` then throw the
/// entity's generated `UniqueViolation`. In the prelude.
@Annotation
public struct Unique {}

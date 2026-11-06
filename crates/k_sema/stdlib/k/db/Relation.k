/// A relation to another entity: a foreign key for a to-one field, the
/// other side of a to-one relation for a to-many field. In the prelude.
@Annotation
public struct Relation {
    /// The field of the target entity this to-many relation is the other
    /// side of, when it isn't unique.
    let inverse: DeclarationName? = nil
}

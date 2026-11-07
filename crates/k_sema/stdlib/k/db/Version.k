/// Marks an optimistic-locking field, `@Version var revision: Int = 1`:
/// updating the entity becomes conditional and returns `Bool`. Requires
/// `var`. In the prelude.
@Annotation
public struct Version {}

/// An optimistic-locking field, `@Version var revision: Int = 1`: `update`
/// becomes conditional and returns `Bool`. Requires `var`. In the prelude.
@Annotation
public struct Version {}

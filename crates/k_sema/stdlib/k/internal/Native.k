/// Marks a standard-library declaration whose implementation is provided by
/// the compiler or the runtime instead of a Kobra body.
///
/// A `@Native` function or computed property has no body, and a `@Native`
/// initializer has an empty body `{}` because the grammar requires one. A
/// `@Native` type's members are all native. Only files of the standard
/// library may use this annotation; user code can't import `k.internal`.
@Annotation
public struct Native {}

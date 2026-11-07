/// Two-way state: the `@Binding` annotation on a component field, and the
/// type of a custom binding (`docs/design/04-components.md`, "Two-way
/// binding").
///
/// `@Binding var value: Int` makes `value` writable by the child and
/// requires `bind x` at the call site. `Binding(get: { … }, set: { v => … })`
/// builds a binding from functions.
@Annotation
@Native
public struct Binding<T> {
    /// Creates a binding that reads with `get` and writes with `set`.
    ///
    /// - Parameters:
    ///   - get: returns the current value.
    ///   - set: stores a new value written by the child.
    @Native
    init(get: () -> T, set: (T) -> Void) {}
}

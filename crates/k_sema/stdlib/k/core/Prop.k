/// Marks a one-way input from the parent component, read-only in the child
/// (`docs/design/04-components.md`). Requires `let`. Without a default it is
/// a required named argument; with one it is a chained option.
@Annotation
public struct Prop {}

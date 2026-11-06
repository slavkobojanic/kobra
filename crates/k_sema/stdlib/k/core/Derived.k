/// A read-only value computed lazily from reactive values, with no
/// dependency list. Requires `let`. Writing it, or writing `@State` while
/// computing it, is a compile error (`docs/design/03-reactivity.md`).
@Annotation
public struct Derived {}

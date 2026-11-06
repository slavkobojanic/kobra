/// A mutable reactive value: writing it updates everything that read it
/// (`docs/design/03-reactivity.md`). Requires `var`. In a service, only the
/// service's own methods may write it.
@Annotation
public struct State {}

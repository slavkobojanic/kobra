/// Declares that a standard-library type is an annotation.
///
/// `@Length(min: 8)` is checked like a call of `Length`'s memberwise
/// initializer, so an annotation's parameters are its stored fields. What an
/// annotation means, where it may appear and which `let`/`var` it requires
/// are defined by the compiler's annotation table
/// (`docs/compiler/sema.md`, "Annotations"). User code can't declare
/// annotations.
@Annotation
public struct Annotation {}

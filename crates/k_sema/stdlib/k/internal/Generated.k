/// Marks a declaration whose cases or members the compiler generates for
/// each app: `Page`, `Endpoint`, `ThemeVariant`, `SessionUser`, `SessionRole`
/// and `FieldName`.
///
/// The declaration in the standard library documents the type and holds the
/// members every app gets; the compiler adds the rest
/// (`docs/compiler/sema.md`, "Generated declarations").
@Annotation
public struct Generated {}

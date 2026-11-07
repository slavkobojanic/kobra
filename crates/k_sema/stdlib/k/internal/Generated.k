/// Marks a declaration whose cases or members the compiler generates for
/// each app: [`k.core.Page`], [`k.core.Endpoint`], [`k.core.ThemeVariant`],
/// [`k.auth.SessionUser`], [`k.auth.SessionRole`] and
/// [`k.validation.FieldName`].
///
/// The declaration in the standard library documents the type and holds the
/// members every app gets; the compiler adds the rest
/// (`docs/compiler/sema.md`, "Generated declarations").
@Annotation
public struct Generated {}

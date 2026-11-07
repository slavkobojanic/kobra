/// Marks an initializer that takes only a string literal, validated at
/// compile time: `Url("https://…")` ([`k.core.Url`]), `Uuid("…")`
/// ([`k.crypto.Uuid`]), `EmailAddress("…")` ([`k.text.EmailAddress`])
/// (`docs/design/20-standard-library.md`, "Literals and parsing").
///
/// The compiler validates the literal with the type's parser and reports
/// any other argument with a quick fix to `Type.parse`.
@Annotation
public struct Literal {}

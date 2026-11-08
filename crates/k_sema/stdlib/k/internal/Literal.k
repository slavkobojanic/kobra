/// Marks an initializer that takes only a literal, validated at compile
/// time: `Url("https://…")` ([`k.core.Url`]), `Uuid("…")`
/// ([`k.crypto.Uuid`]), `EmailAddress("…")` ([`k.text.EmailAddress`]),
/// `TimeOfDay("03:00")` ([`k.time.TimeOfDay`]), `StorageKey(name: "…")`
/// ([`k.storage.StorageKey`]), `DragKind(name: "…")` ([`k.drag.DragKind`])
/// and the integer literal of
/// `HttpStatus(code: 418)` ([`k.http.HttpStatus`])
/// (`docs/design/20-standard-library.md`, "Literals and parsing").
///
/// The compiler validates the literal with the type's parser, the same as
/// the runtime's `Type.parse` (K2404), and reports any other argument
/// (K2403), with a quick fix to `Type.parse` when the type has one.
@Annotation
public struct Literal {}

/// Marks a dependency resolved by the compile-time dependency graph
/// (`docs/design/10-services.md`). Requires `let`. Injected fields are always
/// private; `public` on one is a compile error (K1303).
@Annotation
public struct Inject {}

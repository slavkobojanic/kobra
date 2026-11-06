/// The environment variable a `config` field reads: `@Env("PORT")`
/// (`docs/design/25-config-and-secrets.md`). Requires `let`.
@Annotation
public struct Env {
    /// Names the variable.
    @Native
    init(_ name: String) {}
}

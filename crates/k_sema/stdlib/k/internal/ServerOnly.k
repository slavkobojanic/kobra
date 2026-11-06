/// Marks a standard-library declaration as server-only, like an entity or a
/// config: reaching it from client code is a compile error
/// (`docs/design/10-services.md`, "Server/client boundary").
@Annotation
public struct ServerOnly {}

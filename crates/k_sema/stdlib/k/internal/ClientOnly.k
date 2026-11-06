/// Marks a standard-library declaration as client-only: reaching it from
/// code that runs on the server is a compile error, and a service that
/// injects it is client-only (`docs/design/10-services.md`).
@Annotation
public struct ClientOnly {}

/// Marks a standard-library interface as a platform interface: I/O the
/// platform provides, injectable only into services
/// (`docs/design/10-services.md`, "Layering").
///
/// `side` says where it can be used, which also classifies every service
/// that injects it.
@Annotation
public struct Platform {
    /// Where the interface can be used.
    let side: PlatformSide
}

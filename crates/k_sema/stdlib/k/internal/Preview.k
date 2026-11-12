/// Marks a standard-library declaration as part of a preview feature:
/// `@Preview("workers")`, naming a `preview` row of the feature table
/// (`k_sema::features`, `docs/design/48-releases-and-versioning.md`,
/// "Stability levels"). A project uses it only after listing the feature
/// in `[package] preview` of its `k.toml`; otherwise each use is K8108.
@Annotation
public struct Preview {
    /// Marks the declaration.
    ///
    /// - Parameter feature: the feature's id.
    @Native
    init(_ feature: String) {}
}

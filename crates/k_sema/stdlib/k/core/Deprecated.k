/// Marks a declaration as deprecated:
/// `@Deprecated("use `total(of:)`", since: "0.6", replacement: "total(of:)")`
/// (`docs/design/48-releases-and-versioning.md`, "Deprecation"). It goes
/// on any declaration or member, in an app, a library or the standard
/// library.
///
/// Every use of the declaration is a warning (K1401) naming the version and
/// the message. When `replacement` names a declaration beside this one (a
/// member of the same type, or a declaration of the same module) with the
/// same argument labels, the warning has a fix that switches the use to it,
/// which `k fix` applies; before 1.0 a deprecated declaration is removed in
/// a later minor at the earliest.
@Annotation
public struct Deprecated {
    /// Deprecates the declaration.
    ///
    /// - Parameters:
    ///   - message: what to use instead, or why it is going away.
    ///   - since: the version that deprecated it, such as `"0.6"`.
    ///   - replacement: the replacing declaration's name, with its argument
    ///     labels for a function (`"total(of:)"`); empty when there is none.
    @Native
    init(_ message: String, since: String, replacement: String = "") {}
}

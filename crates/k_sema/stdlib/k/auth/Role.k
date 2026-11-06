/// The guard that refuses users without a role: `requires Role(.admin)`.
/// A compile error when the app declares no `AuthRole` enum.
@Guard
public struct Role {
    /// Requires `role`.
    @Native
    init(_ role: SessionRole) {}
}

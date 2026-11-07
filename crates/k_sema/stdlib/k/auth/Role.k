/// The guard that refuses users without a role: `requires Role(.admin)`.
/// A compile error when the app declares no [`AuthRole`] enum.
@Guard
public struct Role {
    /// Requires `role`.
    ///
    /// - Parameter role: a case of the app's role enum ([`SessionRole`]).
    @Native
    init(_ role: SessionRole) {}
}

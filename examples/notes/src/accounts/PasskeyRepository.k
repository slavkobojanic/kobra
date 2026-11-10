/// Looks up `Passkey`s. New passkeys are stored with the inherited
/// `insert`.
public repository PasskeyRepository for Passkey {
    /// The passkey with a credential id, as a sign-in reports it.
    ///
    /// - Parameter credentialId: the base64url id.
    /// - Returns: the passkey, or `nil` when none has the id.
    func byCredentialId(_ credentialId: String) async -> Passkey? {
        return await passkeys.first { passkey => passkey.credentialId == credentialId }
    }

    /// Writes back what a sign-in changed: the counter, the backup state
    /// and when it was used.
    ///
    /// - Parameters:
    ///   - passkey: the stored passkey.
    ///   - signCount: the verified counter.
    ///   - backedUp: whether the passkey is synced now.
    ///   - at: when it signed in.
    func recordUse(_ passkey: Passkey, signCount: Int, backedUp: Bool, at: Date) async {
        await passkeys
            .filter { p => p.id == passkey.id }
            .update { p =>
                p.signCount = signCount
                p.backedUp = backedUp
                p.lastUsedAt = at
            }
    }
}

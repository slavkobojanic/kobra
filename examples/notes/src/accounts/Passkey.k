import k.auth.PasskeyCredential
import k.crypto.Uuid

/// A passkey that signs an account in, stored in the `passkey` table.
///
/// Server-only. It holds the fields of the `k.auth.PasskeyCredential`
/// that `PasskeyVerifier` returned, because an entity column can't hold a
/// struct; `credential` and `using(_:)` convert both ways.
public entity Passkey {
    /// The primary key.
    @Id let id: Uuid

    /// The account the passkey signs in.
    let accountId: AccountId

    /// The credential id the browser reports at each sign-in, base64url.
    /// Unique, so one passkey can never belong to two accounts.
    @Unique let credentialId: String

    /// The credential's COSE public key, base64url.
    let publicKey: String

    /// The signature counter, written back after each sign-in.
    var signCount: Int

    /// The user handle the device stores with the passkey, base64url.
    let userHandle: String

    /// Whether the passkey may be synced between the person's devices.
    let backupEligible: Bool

    /// Whether it was synced at its last use.
    var backedUp: Bool

    /// When it was registered.
    let createdAt: Date

    /// When it last signed in, `nil` before.
    var lastUsedAt: Date?

    /// The stored credential, for `PasskeyVerifier.verifySignIn`.
    func credential() -> PasskeyCredential {
        return PasskeyCredential(id: credentialId, publicKey: publicKey, signCount: signCount, userHandle: userHandle, backupEligible: backupEligible, backedUp: backedUp)
    }
}

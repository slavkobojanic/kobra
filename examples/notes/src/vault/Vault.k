import accounts.AccountId
import k.crypto.AesGcmKey
import k.crypto.PassphraseSalt
import k.crypto.WrappedKey

/// An account's vault, stored in the `vault` table: one per account.
///
/// The salt and the wrapped key are text columns. Neither is secret: the
/// key inside opens only with the passphrase, which never reaches the
/// server.
public entity Vault {
    /// The account the vault belongs to.
    @Id let ownerId: AccountId

    /// The PBKDF2 salt, with its work factor.
    let salt: PassphraseSalt

    /// The vault key, wrapped with the passphrase's key.
    let wrappedKey: WrappedKey<AesGcmKey>
}

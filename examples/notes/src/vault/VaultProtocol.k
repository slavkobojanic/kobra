import k.auth.Authenticated
import k.crypto.Sealed
import k.crypto.Uuid
import notes.PrivateText

/// The account's private vault (`docs/design/33-crypto.md`, "Example:
/// private notes"). The server sees only the salt, the wrapped key and
/// sealed text: it stores and syncs them but can't read them.
public protocol VaultProtocol requires Authenticated {
    /// The account's vault, or `nil` before one is set up.
    ///
    /// - Throws: `VaultError.rpcFailure` when nobody is signed in.
    rpc get() throws VaultError -> StoredVault?

    /// Stores a new vault.
    ///
    /// - Throws: `VaultError.alreadyCreated` when the account has one.
    rpc create(vault: StoredVault) throws VaultError

    /// A note's sealed private text, or `nil` when it has none.
    ///
    /// - Throws: `VaultError.notFound` for another account's note.
    rpc privateText(note id: Uuid) throws VaultError -> Sealed<PrivateText>?

    /// Replaces a note's sealed private text; `nil` removes it.
    ///
    /// - Throws: `VaultError.notFound` for another account's note.
    rpc setPrivateText(note id: Uuid, text: Sealed<PrivateText>?) throws VaultError
}

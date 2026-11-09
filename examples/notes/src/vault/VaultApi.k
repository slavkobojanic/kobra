import k.auth.Session
import k.crypto.Sealed
import k.crypto.Uuid
import notes.NoteRepository
import notes.PrivateText

/// The server implementation of `VaultProtocol`. It stores what the browser
/// sealed and never holds a key that opens it.
public service VaultApi implements VaultProtocol {
    @Inject let vaults: VaultRepository
    @Inject let notes: NoteRepository
    @Inject let session: Session

    rpc get() async throws VaultError -> StoredVault? {
        guard let vault = await vaults.forOwner(ownerId: session.user.id) else {
            return nil
        }
        return StoredVault(salt: vault.salt, wrappedKey: vault.wrappedKey)
    }

    rpc create(vault: StoredVault) async throws VaultError {
        let ownerId = session.user.id
        guard await vaults.forOwner(ownerId: ownerId) == nil else {
            throw VaultError.alreadyCreated
        }
        await vaults.insert(Vault(ownerId: ownerId, salt: vault.salt, wrappedKey: vault.wrappedKey))
    }

    rpc privateText(note id: Uuid) async throws VaultError -> Sealed<PrivateText>? {
        guard let note = await notes.find(id: id, ownerId: session.user.id) else {
            throw VaultError.notFound
        }
        return note.privateText
    }

    rpc setPrivateText(note id: Uuid, text: Sealed<PrivateText>?) async throws VaultError {
        guard await notes.setPrivateText(id: id, ownerId: session.user.id, text: text) else {
            throw VaultError.notFound
        }
    }
}

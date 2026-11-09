import k.crypto.AesGcmKey
import k.crypto.PassphraseSalt
import k.crypto.Sealed

/// Creates, unlocks and uses a passphrase-protected vault key.
public struct Vault {
    let key: AesGcmKey

    /// Sets up a vault with `passphrase`: a fresh key, wrapped with a key
    /// derived from the passphrase.
    static func create(passphrase: String) async -> CreatedVault {
        let salt = PassphraseSalt.generate()
        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: salt)
        let vault = await AesGcmKey.generateExportable()
        let stored = StoredVault(salt: salt, wrappedKey: await wrapping.wrap(vault))
        return CreatedVault(vault: Vault(key: vault.key), stored: stored)
    }

    /// Unlocks `stored` with `passphrase`.
    static func unlock(_ stored: StoredVault, passphrase: String) async throws VaultError -> Vault {
        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: stored.salt)
        let key = try await wrapping.unwrap(stored.wrappedKey) else {
            throw VaultError.wrongPassphrase
        }
        return Vault(key: key)
    }

    /// Seals `text` for the note `id`.
    func seal(_ text: PrivateText, note id: Int) async -> Sealed<PrivateText> {
        return await key.seal(text, context: "note:{id}".utf8)
    }

    /// Opens what was sealed for the note `id`.
    func open(_ sealed: Sealed<PrivateText>, note id: Int) async throws VaultError -> PrivateText {
        do {
            return try await key.open(sealed, context: "note:{id}".utf8)
        } catch .authenticationFailed, .undecodable {
            throw VaultError.tampered
        }
    }
}

import k.crypto.AesGcmKey
import k.crypto.KeyName
import k.crypto.KeyStore
import k.crypto.PassphraseSalt
import k.crypto.Sealed
import k.crypto.Uuid
import notes.PrivateText

/// Unlocks the vault in this browser and seals and opens private notes.
///
/// Client-only, because it injects `KeyStore`. The vault key is generated
/// here, never exported in the clear, and never sent to the server: its
/// type isn't `Codable`, so no rpc could carry it.
public service VaultService {
    @Inject let api: VaultProtocol
    @Inject let keys: KeyStore

    private static let vaultKey: KeyName<AesGcmKey> = KeyName(name: "notes-vault")

    @State var state: VaultState = .unknown

    /// Restores the key this browser kept, if any. The private note page
    /// calls it from an effect, because `init()` can't await.
    func restore() async {
        let kept = try await keys.get(VaultService.vaultKey) else { nil }
        if let kept {
            state = .unlocked(kept)
        } else {
            state = .locked
        }
    }

    /// Whether the account has a vault yet, to offer setting one up.
    func exists() async throws VaultError -> Bool {
        return try await api.get() != nil
    }

    /// Sets up a vault the first time, with a passphrase that isn't the
    /// sign-in password.
    func create(passphrase: String) async throws VaultError {
        let salt = PassphraseSalt.generate()
        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: salt)
        let vault = await AesGcmKey.generateExportable()

        try await api.create(vault: StoredVault(salt: salt, wrappedKey: await wrapping.wrap(vault)))
        await remember(vault.key)
    }

    /// Unlocks on a new browser.
    func unlock(passphrase: String) async throws VaultError {
        guard let stored = try await api.get() else {
            throw VaultError.noVault
        }

        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: stored.salt)

        let key = try await wrapping.unwrap(stored.wrappedKey) else {
            throw VaultError.wrongPassphrase
        }
        await remember(key)
    }

    func seal(_ text: PrivateText, note id: Uuid) async throws VaultError -> Sealed<PrivateText> {
        let key = try unlockedKey()
        return await key.seal(text, context: "note:{id}".utf8)
    }

    func open(_ sealed: Sealed<PrivateText>, note id: Uuid) async throws VaultError -> PrivateText {
        let key = try unlockedKey()
        do {
            return try await key.open(sealed, context: "note:{id}".utf8)
        } catch .authenticationFailed, .undecodable {
            throw VaultError.tampered
        }
    }

    /// A note's private text, opened; empty when it has none.
    func read(note id: Uuid) async throws VaultError -> PrivateText {
        guard let sealed = try await api.privateText(note: id) else {
            return PrivateText(body: "")
        }
        return try await open(sealed, note: id)
    }

    /// Seals and stores a note's private text; an empty text removes it.
    func write(_ text: PrivateText, note id: Uuid) async throws VaultError {
        if text.body.isEmpty {
            try await api.setPrivateText(note: id, text: nil)
        } else {
            try await api.setPrivateText(note: id, text: try await seal(text, note: id))
        }
    }

    /// Forgets the key in this browser, for example at sign-out.
    func lock() async {
        state = .locked
        try await keys.delete(VaultService.vaultKey) else { return }
    }

    private func unlockedKey() throws VaultError -> AesGcmKey {
        return match state {
            case .unlocked(let key) { key }
            case .locked, .unknown { throw VaultError.locked }
        }
    }

    private func remember(_ key: AesGcmKey) async {
        do {
            try await keys.put(key, as: VaultService.vaultKey)
        } catch .unavailable {
            // Keep it for this tab only; the person unlocks again next visit.
        }
        state = .unlocked(key)
    }
}

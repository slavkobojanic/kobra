// With `@Breaking`, a key name may be renamed: the service makes a new key
// when `get` finds none. The other key name matches its entry.
import k.crypto.AesGcmKey
import k.crypto.KeyName
import k.crypto.KeyStore

/// Keeps the vault key in this browser.
public service VaultService {
    @Inject let keys: KeyStore

    @Breaking("the vault is unlocked again with the passphrase")
    private static let vaultKey = KeyName<AesGcmKey>(name: "vault")

    private let backupKey: KeyName<AesGcmKey> = KeyName<AesGcmKey>(name: "notes-backup")
}

// Renaming a key name leaves the key every browser kept under the old
// name: K8010, found by the recorded declaration.
import k.crypto.AesGcmKey
import k.crypto.KeyName
import k.crypto.KeyStore

/// Keeps the vault key in this browser.
public service VaultService {
    @Inject let keys: KeyStore

    private static let vaultKey = KeyName<AesGcmKey>(name: "vault")
}

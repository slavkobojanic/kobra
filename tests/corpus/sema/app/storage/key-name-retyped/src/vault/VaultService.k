// Another key type is K8010 even with `@Breaking`: a stored key keeps the
// type it was written with, and `get` would return it as the new one.
import k.crypto.Ed25519PrivateKey
import k.crypto.KeyName
import k.crypto.KeyStore

/// Keeps the vault key in this browser.
public service VaultService {
    @Inject let keys: KeyStore

    @Breaking("a signing key now")
    private static let vaultKey = KeyName<Ed25519PrivateKey>(name: "notes-vault")
}

// A key in a config (33-crypto.md, "Keys in configuration"): a secret or private key is held as
// `Secret<K>`, so it may come from `NAME_FILE` and is never printed. A bare one is K4032, with a
// fix that wraps the type (keeping `?`); public keys are plain `ConfigValue`s.
import k.crypto.AesGcmKey
import k.crypto.Ed25519PrivateKey
import k.crypto.Ed25519PublicKey
import k.crypto.RsaPublicKey

config SigningConfig {
    @Env("SIGNING_KEY") let signingKey: Ed25519PrivateKey
    @Env("BACKUP_KEY") let backupKey: AesGcmKey?
    @Env("OLD_KEYS") let oldKeys: List<Ed25519PrivateKey>
    @Env("WRAPPED_KEY") let wrapped: Secret<Ed25519PrivateKey>
    @Env("PEER_KEY") let peerKey: Ed25519PublicKey
    @Env("IDP_KEY") let idpKey: RsaPublicKey?
}

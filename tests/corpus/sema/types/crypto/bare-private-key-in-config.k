// A key in a config (33-crypto.md, "Keys in configuration"): the key types aren't `ConfigValue`
// yet, so a bare private key field is the field-type error. Reading keys from configuration,
// and the quick fix that wraps a bare private key in `Secret<K>`, are Planned (crypto-config-keys).
import k.crypto.Ed25519PrivateKey

config SigningConfig {
    @Env("SIGNING_KEY") let signingKey: Ed25519PrivateKey
}

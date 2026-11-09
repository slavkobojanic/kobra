// A plain key has no export members (33-crypto.md, "Exportable keys"): whether a key can leave
// is part of its type, never a flag checked at run time. `Exportable<K>` exports, as a
// `Secret<Bytes>`, and wrapping takes only an `Exportable<K>`.
import k.crypto.AesGcmKey
import k.crypto.Ed25519PrivateKey
import k.crypto.WrappedKey

func export(vault: AesGcmKey, signer: Ed25519PrivateKey) async -> Secret<Bytes> {
    let raw = vault.exportRaw()
    let seed = signer.exportRaw()
    let wrapped: WrappedKey<Ed25519PrivateKey> = await vault.wrap(signer)
    let exportable = await AesGcmKey.generateExportable()
    return exportable.exportRaw()
}

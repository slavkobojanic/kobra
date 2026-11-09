// `k.crypto`'s key types on the server's ops (33-crypto.md): RFC 8032's first key and signature
// through `Ed25519PrivateKey.parse(raw:)` and `sign`, RFC 5869's third case through
// `Hkdf.sha256`, a sealed value that opens with its context and not with another, a
// configured key's forwarded `seal`, and the constant-time comparison.
import k.crypto.AesGcmKey
import k.crypto.ConstantTime
import k.crypto.Ed25519PrivateKey
import k.crypto.Hash
import k.crypto.Hkdf
import k.crypto.Sealed

func main() async -> List<String> {
    let seed = Bytes.parse(hex: "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60") ?? "".utf8
    guard let key = Ed25519PrivateKey.parse(raw: Secret(seed)) else {
        return ["no key"]
    }
    let signature = await key.sign("".utf8)
    let ikm = Bytes.parse(hex: "0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b") ?? "".utf8
    let okm = await Hkdf.sha256(from: Secret(ikm), salt: "".utf8, info: "", count: 42)
    let aes = Secret(await AesGcmKey.generate())
    let sealed: Sealed<String> = await aes.seal("hi", context: "a".utf8)
    let opened = try await aes.open(sealed, context: "a".utf8) else { "refused" }
    let moved = try await aes.open(sealed, context: "b".utf8) else { "refused" }
    let digest = await Hash.sha256(data: "abc".utf8)
    return [
        key.publicKey.raw.hex,
        signature.bytes.hex,
        okm.reveal().hex,
        opened,
        moved,
        "{ConstantTime.equals("a".utf8, "a".utf8)} {ConstantTime.equals("a".utf8, "b".utf8)}",
        digest.hex,
        "{key}",
    ]
}

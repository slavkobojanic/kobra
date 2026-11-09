// Secret and private keys and shared secrets aren't `Equatable` (33-crypto.md, "Comparing in
// constant time"): `==` on them is the operator error, with a hint naming what to compare
// instead. Public keys, signatures and sealed values are ordinary data and compare.
import k.crypto.AesGcmKey
import k.crypto.Ed25519PrivateKey
import k.crypto.Sealed
import k.crypto.SharedSecret
import k.crypto.X25519PrivateKey

func compare(a: AesGcmKey, b: AesGcmKey, s: Ed25519PrivateKey, t: Ed25519PrivateKey, x: SharedSecret, y: SharedSecret, p: X25519PrivateKey, q: Sealed<String>) -> Bool {
    let keys = a == b
    let signers = s == t
    let shared = x == y
    let publicKeys = s.publicKey == t.publicKey and p.publicKey != p.publicKey
    let sealed = q == q
    return keys or signers or shared or publicKeys or sealed
}

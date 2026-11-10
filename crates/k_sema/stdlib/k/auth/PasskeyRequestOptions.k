/// What the browser needs to sign in with a passkey
/// (`docs/design/51-passkeys.md`), from
/// [`PasskeyVerifier.signInOptions(allowing:)`] to
/// [`Passkeys.signIn(_:)`]. `Codable`, so it crosses an rpc.
public struct PasskeyRequestOptions implements Equatable, Codable {
    /// The challenge, base64url: issued into the server's session,
    /// single-use, valid for ten minutes.
    let challenge: String
    /// The relying party's id: the host of `K_PUBLIC_ORIGIN`.
    let rpId: String
    /// The credential ids, base64url, the device may use; empty lets it
    /// offer every passkey it holds for the site.
    let allowed: List<String>
}

/// What the browser needs to make a passkey
/// (`docs/design/51-passkeys.md`), from
/// [`PasskeyVerifier.registrationOptions(userHandle:excluding:)`] to
/// [`Passkeys.register(_:name:)`]. `Codable`, so it crosses an rpc.
public struct PasskeyCreationOptions implements Equatable, Codable {
    /// The challenge, base64url: issued into the server's session,
    /// single-use, valid for ten minutes.
    let challenge: String
    /// The relying party's id: the host of `K_PUBLIC_ORIGIN`.
    let rpId: String
    /// The relying party's name the device shows: the app's title.
    let rpName: String
    /// The user handle the device stores with the passkey, base64url.
    let userHandle: String
    /// Credential ids, base64url, that the device must not register again.
    let excluded: List<String>
}

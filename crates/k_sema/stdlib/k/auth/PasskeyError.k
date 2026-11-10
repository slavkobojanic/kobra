/// Why a passkey ceremony in the browser ended without a result
/// (`docs/design/51-passkeys.md`).
public enum PasskeyError implements Error, Equatable {
    /// This browser or context has no WebAuthn.
    case unsupported
    /// The sheet was dismissed or timed out, or the device has no passkey
    /// for this site. Browsers report these alike, so a site can't probe
    /// which passkeys a device holds.
    case cancelled
    /// The device already holds one of the credentials the options
    /// exclude: this account's passkey is already on it.
    case alreadyRegistered
    /// Anything else, such as an RP id the page's origin can't use.
    case failed
}

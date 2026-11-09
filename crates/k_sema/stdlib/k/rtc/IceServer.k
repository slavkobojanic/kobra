/// A STUN or TURN server a browser may use to reach its peer
/// (`docs/design/46-media-and-rtc.md`, "STUN and TURN without third-party
/// accounts"), as [`IceServers.forClient(user:validFor:)`] issues them.
public struct IceServer implements Codable, Equatable {
    /// `stun:`, `turn:` and `turns:` URLs (RFC 7064 and 7065). They have
    /// no `//` and no `http` scheme, so they are text, not `Url`s;
    /// [`TurnConfig`] checks their schemes at startup.
    let urls: List<String>
    /// The TURN user name, `<expiry>:<user>`.
    let username: String? = nil
    /// The short-lived TURN credential. It is meant for the browser and
    /// expires, so it isn't a `Secret`.
    let credential: String? = nil
}

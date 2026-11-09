/// Issues the ICE servers a browser connects through, with short-lived
/// TURN credentials (`docs/design/46-media-and-rtc.md`, "STUN and TURN
/// without third-party accounts"). Server-only: the secret they are signed
/// with stays in [`TurnConfig`].
///
/// There is no default STUN or TURN server: with no `K_TURN_URLS` it
/// issues none, and browsers on one machine or one network reach each
/// other directly.
@ServerOnly
@Native
public service IceServers {
    /// The servers for `user`, valid for `validFor`: one entry with every
    /// `K_TURN_URLS` URL and a credential in coturn's TURN REST scheme (the
    /// user name `<expiry>:<user>`, the credential the Base64 HMAC-SHA1 of
    /// the user name under `K_TURN_SECRET`), or none when no URL is set.
    func forClient(user: String, validFor: Duration) async -> List<IceServer>
}

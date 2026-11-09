/// Pairs the peers that join under one key and forwards each one's
/// [`Signal`]s to the other (`docs/design/46-media-and-rtc.md`,
/// "Signalling over a channel"). Held by a `@Scope(.server)` service:
///
/// ```k,ignore
/// let relay: SignallingRelay<RoomId> = SignallingRelay(peersPerRoom: 2)
/// ```
///
/// It is built on [`k.rpc.Broadcast`] and [`k.rpc.Presence`], so the
/// peers of a room may be on different replicas, and a peer is a lease of
/// the request that joined: it leaves when its channel closes.
@ServerOnly
@Native
public class SignallingRelay<Key> where Key implements Hashable, Key implements Codable {
    /// Creates a relay admitting `peersPerRoom` peers under each key.
    @Native
    init(peersPerRoom: Int = 2) {}

    /// Joins the room `key` for as long as the calling request lives; for a
    /// channel's handler, until the channel closes.
    ///
    /// - Returns: the peer, whose `signals` the handler sends and whose
    ///   `forward(_:)` its receiving function calls, or `nil` when the room
    ///   already has `peersPerRoom` peers.
    func join(key: Key) async -> RelayPeer?
}

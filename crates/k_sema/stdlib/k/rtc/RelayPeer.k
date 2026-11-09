/// One peer of a [`SignallingRelay`] room, a lease of the request that
/// joined (`docs/design/46-media-and-rtc.md`, "Signalling over a channel").
@ServerOnly
@Native
public class RelayPeer {
    /// What this peer receives: the other peer's signals, and the relay's
    /// own `peerJoined` and `peerLeft`. The handler's `sending` stream.
    let signals: Stream<Signal>

    /// Forwards one of this peer's signals to the other peers of its room.
    func forward(_ signal: Signal)
    /// Leaves the room now instead of when the request ends; idempotent.
    func leave()
}

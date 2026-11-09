import k.media.MediaStream

/// A call with one peer (`docs/design/46-media-and-rtc.md`,
/// "`PeerConnections` and `PeerConnection`"). Its fields are reactive.
///
/// Negotiation is the stdlib's ("perfect negotiation"): adding a stream,
/// opening a data channel and restarting ICE after a network change need
/// no app code. A handle, not a value; it closes with its owner.
@ClientOnly
@Native
public class PeerConnection {
    /// `.waitingForPeer`, `.connecting`, `.connected`, `.reconnecting` or
    /// `.closed` and why.
    var state: PeerState
    /// What the peer sends; `nil` until its first track arrives.
    var remote: MediaStream?

    /// Sends every track of `stream` to the peer.
    ///
    /// - Returns: the sender, to replace the tracks or stop sending them.
    func send(_ stream: MediaStream) -> MediaSender
    /// A typed data channel named by `kind`. Both peers open it, in any
    /// order.
    ///
    /// - Parameter delivery: `.reliable` (ordered, retransmitted), or
    ///   `.unordered(retransmits:)` where only the latest matters.
    func channel<T>(_ kind: DataChannelKind<T>, delivery: Delivery = .reliable) -> DataChannel<T>
    /// Round-trip time, bitrate, packet loss, and whether a relay is in
    /// use.
    func stats() async -> PeerStats
    /// Hangs up: the peer sees `.closed(.byPeer)`. Closing a closed
    /// connection does nothing.
    func close()
}

import k.media.MediaStream
import k.rtc.DataChannel
import k.rtc.DataChannelKind
import k.rtc.MediaSender
import k.rtc.PeerState

/// The other side of a call in a test ([`TestPeerConnections`]): a peer
/// connection the test drives, joined through the app's own protocol.
@Native
public class TestCallPeer {
    /// Where the connection is.
    var state: PeerState
    /// What the app sends. Its tracks are the app's own, so a test can
    /// check that muting disabled them.
    var remote: MediaStream?

    /// Sends `stream` to the app, as [`k.rtc.PeerConnection.send(_:)`].
    func send(_ stream: MediaStream) -> MediaSender
    /// Hangs up: the app sees `.closed(.byPeer)`.
    func close()
    /// The test peer's end of a data channel, to send to the app.
    func channel<T>(_ kind: DataChannelKind<T>) -> DataChannel<T>
    /// Every message the app sent on `kind`'s channel, oldest first.
    func received<T>(_ kind: DataChannelKind<T>) -> List<T>
}

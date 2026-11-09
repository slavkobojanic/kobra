/// One typed data channel of a [`PeerConnection`]
/// (`docs/design/46-media-and-rtc.md`, "Typed data channels"). Messages
/// are encoded like rpc messages, and each side checks that the other's
/// type has the same shape when it opens.
@ClientOnly
@Native
public class DataChannel<T> {
    /// Every message the peer sends, in order for `.reliable`; ends when
    /// the channel or its connection closes. Read once.
    let messages: Stream<T>
    /// Whether messages can be sent now; reactive.
    var isOpen: Bool

    /// Sends `message`, waiting first while more than 1 MiB is buffered or
    /// until the channel opens.
    ///
    /// - Throws: [`DataChannelError.tooLarge`] over 64 KiB encoded,
    ///   `.incompatible` when the peer's message type differs, `.closed`.
    func send(_ message: T) async throws DataChannelError
    /// Closes this channel.
    func close()
}

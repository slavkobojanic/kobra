import k.media.MediaStream

/// The tracks a [`PeerConnection`] sends from one stream.
@ClientOnly
@Native
public class MediaSender {
    /// Sends `stream`'s tracks instead, with no renegotiation where the
    /// browser allows it: switching cameras, starting a screen share.
    func replace(with stream: MediaStream)
    /// Stops sending these tracks; the peer's copies end with
    /// `.remoteEnded`.
    func stop()
}

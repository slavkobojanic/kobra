/// Live audio and video: what a capture returns
/// (`docs/design/46-media-and-rtc.md`, "Streams and tracks").
///
/// A handle, not a value: it isn't `Codable`, so it can't be stored, sent
/// in an rpc or put in a `@Load` field, and two streams are equal only
/// when they are the same stream. Its fields are reactive, like `@State`.
/// It stops with its owner (`CaptureLifetime`).
@ClientOnly
@Native
public class MediaStream implements Equatable {
    /// The audio tracks.
    var audioTracks: List<MediaTrack>
    /// The video tracks.
    var videoTracks: List<MediaTrack>
    /// Whether some track is still live.
    var isLive: Bool
    /// Whether it was captured on this device, rather than received from a
    /// peer.
    var isLocal: Bool

    /// Stops every track, which releases the devices. Stopping a stopped
    /// stream does nothing.
    func stop()
}

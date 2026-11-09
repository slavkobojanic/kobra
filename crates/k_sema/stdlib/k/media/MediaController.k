/// Drives one `video` or `audio` element from code
/// (`docs/design/46-media-and-rtc.md`, "MediaController"):
/// `audio(source: .file(memo.file)).controller(player)`. Its fields are
/// reactive. Giving it to a second element is a run-time error naming
/// both.
@ClientOnly
@Native
public class MediaController {
    /// Creates a controller driving no element yet.
    @Native
    init() {}

    /// `.idle` until its element has a source, then what it does.
    var state: PlaybackState
    /// The position; reactive.
    var currentTime: Duration
    /// The length; `nil` until known, and for live streams.
    var duration: Duration?
    /// The volume, `0.0...1.0`.
    var volume: Double

    /// Starts playing.
    ///
    /// - Throws: [`PlaybackError.blocked`] when the browser wants a user's
    ///   action first, `.unsupportedFormat` or `.unreachable`.
    func play() async throws PlaybackError
    /// Pauses.
    func pause()
    /// Plays when paused, and pauses when playing. A refused play leaves
    /// the state `.blocked`.
    func toggle() async
    /// Moves the position.
    func seek(to time: Duration)
    /// Sets [`volume`], clamped to `0.0...1.0`.
    func setVolume(_ volume: Double)
}

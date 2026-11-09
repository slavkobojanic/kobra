/// What the browser chose for a [`MediaTrack`]; each is `nil` where it
/// doesn't apply or the browser doesn't say.
public struct TrackSettings implements Equatable {
    /// A video's size.
    let size: VideoSize? = nil
    /// A video's frames per second.
    let frameRate: Double? = nil
    /// An audio's samples per second.
    let sampleRate: Int? = nil
    /// An audio's channels.
    let channelCount: Int? = nil
    /// Whether echo cancellation is on.
    let echoCancellation: Bool? = nil
    /// Whether noise suppression is on.
    let noiseSuppression: Bool? = nil
    /// Whether automatic gain is on.
    let autoGainControl: Bool? = nil
}

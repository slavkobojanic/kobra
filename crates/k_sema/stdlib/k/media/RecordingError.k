/// Why a recording couldn't start or finish.
public enum RecordingError implements Error, Equatable {
    /// The browser can't record this `.exact` media type.
    case unsupportedFormat
    /// The stream has no track the format records: `.voice` on a stream
    /// without audio.
    case noTrack
    /// The browser's encoder failed.
    case failed
}

/// Why a meter couldn't be made.
public enum AudioError implements Error, Equatable {
    /// This browser has no Web Audio.
    case unsupported
    /// The stream has no audio track.
    case noTrack
}

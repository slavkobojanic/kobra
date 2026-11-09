/// Whether a [`MediaTrack`] still delivers.
public enum TrackState implements Equatable {
    /// It delivers.
    case live
    /// It ended, for good.
    case ended(TrackEnd)
}

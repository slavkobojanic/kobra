/// Why a [`MediaTrack`] ended.
public enum TrackEnd implements Equatable {
    /// The app stopped it, or its owner went away.
    case stopped
    /// The device was unplugged.
    case deviceLost
    /// The browser's own "Stop sharing" button, or a permission revoked
    /// while capturing.
    case endedByUser
    /// The peer stopped sending it.
    case remoteEnded
}

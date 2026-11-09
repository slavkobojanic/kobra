/// What a [`MediaController`]'s element is doing.
public enum PlaybackState implements Equatable {
    /// It has nothing to play yet.
    case idle
    /// It is loading what it plays.
    case loading
    /// It is playing.
    case playing
    /// It is paused.
    case paused
    /// It played to the end.
    case ended
    /// The browser refused to start it without a user's action.
    case blocked
}

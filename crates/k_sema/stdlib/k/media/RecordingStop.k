/// Why a recording stopped.
public enum RecordingStop implements Equatable {
    /// The app called `stop`.
    case byApp
    /// Its stream ended: the microphone was unplugged, or the user stopped
    /// sharing.
    case streamEnded
    /// It reached its [`RecordingLimit`].
    case limitReached
}

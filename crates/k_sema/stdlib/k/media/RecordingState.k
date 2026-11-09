/// What a [`RecordingSession`] is doing.
public enum RecordingState implements Equatable {
    /// It records.
    case recording
    /// It is paused.
    case paused
    /// It stopped, for good.
    case stopped(RecordingStop)
}

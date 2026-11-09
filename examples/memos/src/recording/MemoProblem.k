import k.media.CaptureError
import k.media.RecordingError

/// Why there is no new memo.
public enum MemoProblem implements Equatable {
    /// The user or the browser blocked the microphone.
    case microphoneBlocked
    /// There is no microphone, or the chosen one is gone.
    case noMicrophone
    /// Another application holds the microphone.
    case microphoneInUse
    /// Any other capture failure.
    case cannotCapture(CaptureError)
    /// The recording failed.
    case cannotRecord(RecordingError)
    /// The server didn't store the memo.
    case notSaved
}

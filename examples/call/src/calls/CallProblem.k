import k.media.CaptureError

/// Why the call couldn't start, or stopped.
public enum CallProblem implements Equatable {
    /// The user or the browser blocked the camera or the microphone.
    case devicesBlocked
    /// There is no camera or microphone.
    case noDevices
    /// Another application holds the camera or the microphone.
    case devicesInUse
    /// Any other capture failure.
    case cannotCapture(CaptureError)
    /// This browser can't make calls.
    case unsupported
    /// Two people are in this call already.
    case roomFull
    /// The server couldn't be reached.
    case offline
    /// The screen couldn't be shared.
    case cannotShare
}

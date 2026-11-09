/// How to capture a microphone.
public struct AudioCapture implements Equatable {
    /// The microphone; `nil` is the browser's default. A chosen device is
    /// exact: when it is gone the capture throws `.notFound`.
    let device: MediaDeviceId? = nil
    /// Removes the speakers' sound from the microphone's.
    let echoCancellation: Bool = true
    /// Removes steady background noise.
    let noiseSuppression: Bool = true
    /// Keeps the level steady.
    let autoGainControl: Bool = true
}

/// What a [`MediaDevice`] is.
public enum MediaDeviceKind implements Equatable {
    /// An audio input.
    case microphone
    /// A video input.
    case camera
    /// An audio output.
    case speaker
}

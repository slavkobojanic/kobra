/// What a capture opens (`docs/design/46-media-and-rtc.md`, "Requests"):
/// a request for nothing can't be written.
public enum CaptureRequest implements Equatable {
    /// A microphone.
    case audio(AudioCapture)
    /// A camera.
    case video(VideoCapture)
    /// A microphone and a camera, asked for in one prompt.
    case audioAndVideo(AudioCapture, VideoCapture)

    /// The default microphone, with echo cancellation, noise suppression
    /// and automatic gain.
    static let microphone: CaptureRequest = .audio(AudioCapture())
    /// The default camera.
    static let camera: CaptureRequest = .video(VideoCapture())
    /// The default camera and microphone.
    static let cameraAndMicrophone: CaptureRequest = .audioAndVideo(AudioCapture(), VideoCapture())
}

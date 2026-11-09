/// How to capture a camera. Sizes and frame rates are ideals: the browser
/// picks the nearest it can, and the track's `settings` say what it chose.
public struct VideoCapture implements Equatable {
    /// The camera; `nil` is the browser's default. A chosen device is
    /// exact: when it is gone the capture throws `.notFound`.
    let device: MediaDeviceId? = nil
    /// A phone's front (`.user`) or back (`.environment`) camera.
    let facing: CameraFacing? = nil
    /// The ideal size.
    let size: VideoSize? = nil
    /// The ideal frames per second.
    let frameRate: Int? = nil
}

/// A microphone, camera or speaker the browser lists
/// (`docs/design/46-media-and-rtc.md`, "Capture").
public struct MediaDevice implements Equatable {
    /// The device's id, stable for this site until the user clears its
    /// data.
    let id: MediaDeviceId
    /// What it is.
    let kind: MediaDeviceKind
    /// What the browser calls it ("MacBook Pro Microphone"); `nil` until
    /// the user has granted a capture, since browsers hide labels before.
    let label: String?
}

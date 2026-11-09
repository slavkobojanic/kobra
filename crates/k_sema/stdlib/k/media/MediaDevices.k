import k.device.PermissionState

/// The camera, the microphone and the screen
/// (`docs/design/46-media-and-rtc.md`, "Capture"). A client-only platform
/// interface: [`BrowserMediaDevices`] is injected with no `use` rule, and
/// [`k.test.TestMediaDevices`] replaces it in tests.
///
/// Only [`requestAccess(for:)`] prompts for the camera or the microphone,
/// and only from a user's action. [`capture(_:lifetime:)`] never prompts:
/// while the kind's permission is `.prompt` it throws
/// [`CaptureError.notAsked`].
@Platform(side: .client)
public interface MediaDevices {
    /// Whether this browser can capture here: `false` outside a secure
    /// context (`http:` on a host other than `localhost`), and during
    /// server rendering.
    var isSupported: Bool
    /// Whether the app may use the camera. Reactive; `.prompt` during
    /// server rendering.
    var cameraPermission: PermissionState
    /// Whether the app may use the microphone. Reactive; `.prompt` during
    /// server rendering.
    var microphonePermission: PermissionState
    /// The microphones, cameras and speakers, refreshed when one is plugged
    /// in or removed and after a grant. Empty during server rendering and
    /// until the first enumeration, shortly after hydration.
    var devices: List<MediaDevice>

    /// Asks the user for every kind in `request` in one prompt, unless the
    /// permissions already say `.granted` or `.denied`.
    ///
    /// - Returns: `.granted` when every kind was granted, `.denied` when
    ///   any was refused, `.prompt` when the user dismissed the prompt.
    @UserAction
    func requestAccess(for request: CaptureRequest) async -> PermissionState

    /// Opens the devices `request` names, without prompting.
    ///
    /// - Parameter lifetime: `.owner` stops the stream when the run that
    ///   captured it is cancelled; `.service` when the injecting service's
    ///   instance ends.
    /// - Throws: [`CaptureError.notAsked`] while a kind's permission is
    ///   `.prompt`, `.denied`, `.notFound`, `.busy`, `.unsatisfiable` or
    ///   `.unsupported`.
    func capture(_ request: CaptureRequest, lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream

    /// Shows the browser's screen picker and captures what the user
    /// chooses. It is the request and the capture at once, so it is a
    /// user's action.
    ///
    /// - Throws: [`CaptureError.cancelled`] when the picker was closed
    ///   without a choice, `.unsupported` where screens can't be captured.
    @UserAction
    func captureScreen(_ request: ScreenCapture = ScreenCapture(), lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream
}

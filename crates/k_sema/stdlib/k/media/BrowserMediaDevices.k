import k.device.PermissionState

/// The standard [`MediaDevices`]: the browser's `getUserMedia`,
/// `getDisplayMedia` and `enumerateDevices`, with the permissions read
/// from the Permissions API.
@ClientOnly
@Native
public service BrowserMediaDevices implements MediaDevices {
    /// See [`MediaDevices.isSupported`].
    var isSupported: Bool
    /// See [`MediaDevices.cameraPermission`].
    var cameraPermission: PermissionState
    /// See [`MediaDevices.microphonePermission`].
    var microphonePermission: PermissionState
    /// See [`MediaDevices.devices`].
    var devices: List<MediaDevice>

    /// Opens the devices once, which shows the browser's prompt; see
    /// [`MediaDevices.requestAccess(for:)`].
    @UserAction
    func requestAccess(for request: CaptureRequest) async -> PermissionState
    /// See [`MediaDevices.capture(_:lifetime:)`].
    func capture(_ request: CaptureRequest, lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream
    /// See [`MediaDevices.captureScreen(_:lifetime:)`].
    @UserAction
    func captureScreen(_ request: ScreenCapture = ScreenCapture(), lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream
}

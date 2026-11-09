import k.device.PermissionState
import k.media.CaptureError
import k.media.CaptureKind
import k.media.CaptureLifetime
import k.media.CaptureRequest
import k.media.MediaDevice
import k.media.MediaDeviceId
import k.media.MediaDevices
import k.media.MediaStream
import k.media.MediaTrack
import k.media.ScreenCapture

/// The default [`MediaDevices`] in `k test`
/// (`docs/design/46-media-and-rtc.md`, "Tests"): no prompt appears and no
/// device opens. There is one test microphone, camera and speaker; every
/// permission starts at `.prompt`, the user answers prompts with
/// `.granted`, and captures return synthetic streams.
@TestDefault(replaces: MediaDevices)
@Native
public service TestMediaDevices implements MediaDevices {
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// The camera's permission; reactive.
    var cameraPermission: PermissionState
    /// The microphone's permission; reactive.
    var microphonePermission: PermissionState
    /// The plugged-in devices: `test-microphone`, `test-camera` and
    /// `test-speaker`, labelled once a capture was granted.
    var devices: List<MediaDevice>
    /// The number of [`requestAccess(for:)`] calls that showed a prompt.
    var prompts: Int
    /// Every request made to [`capture(_:lifetime:)`], oldest first.
    var captures: List<CaptureRequest>
    /// The tracks not yet stopped, to check that leaving a page released
    /// the devices.
    var liveTracks: List<MediaTrack>
    /// What the default microphone produces.
    var microphone: TestSource
    /// What the default camera produces.
    var camera: TestSource

    /// Asks while a requested kind is `.prompt`, counting it in
    /// [`prompts`] and answering what [`answerPrompts(with:)`] set.
    func requestAccess(for request: CaptureRequest) async -> PermissionState
    /// A synthetic stream of the requested devices, or what
    /// [`failCaptures(with:)`] set, `.notAsked` or `.denied`.
    func capture(_ request: CaptureRequest, lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream
    /// A synthetic screen stream, or `.cancelled` after
    /// [`cancelScreenPicker()`].
    func captureScreen(_ request: ScreenCapture = ScreenCapture(), lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream

    /// Changes a permission as the site's settings would; views reading it
    /// update.
    func setPermission(_ kind: CaptureKind, to state: PermissionState)
    /// How the simulated user answers the next prompts; `.granted` by
    /// default.
    func answerPrompts(with answer: PermissionState)
    /// Makes every following capture throw `error`.
    func failCaptures(with error: CaptureError)
    /// Makes the next [`captureScreen(_:lifetime:)`] throw `.cancelled`.
    func cancelScreenPicker()
    /// Plugs in a device.
    func add(_ device: MediaDevice)
    /// Unplugs a device: its live tracks end with `.deviceLost`.
    func remove(_ id: MediaDeviceId)
    /// Sets [`isSupported`]; while it is `false` every capture throws
    /// `.unsupported`.
    func setSupported(_ value: Bool)
}

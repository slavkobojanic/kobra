import k.device.Geolocation
import k.device.LocationAccuracy
import k.device.LocationError
import k.device.PermissionState
import k.device.Position

/// The default [`Geolocation`] in `k test`
/// (`docs/design/45-web-platform.md`, "Geolocation"): no prompt opens and
/// the machine's position is never read. The browser is capable, the
/// permission starts at `.prompt`, the user answers prompts with
/// `.granted`, and the device has no fix until [`move(to:)`].
@TestDefault(replaces: Geolocation)
@Native
public service TestGeolocation implements Geolocation {
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// `.prompt` until a prompt is answered or [`setPermission(_:)`]
    /// changes it. Reactive, like the browser's.
    var permission: PermissionState
    /// The number of [`requestAccess()`] calls that asked the user.
    var prompts: Int
    /// The accuracy of the latest [`currentPosition`] or [`watch`]; `nil`
    /// before either.
    var lastAccuracy: LocationAccuracy?

    /// Asks while [`permission`] is `.prompt`, counting it in [`prompts`]
    /// and answering what [`answerPrompts(with:)`] set; otherwise answers
    /// the permission as it is.
    func requestAccess() async -> PermissionState
    /// The position set by [`move(to:)`], or what [`failNext(error:)`] set,
    /// `.unsupported`, `.notAsked`, `.denied`, or `.unavailable` before
    /// the device has moved.
    func currentPosition(accuracy: LocationAccuracy = .coarse, maximumAge: Duration = 0.seconds, timeout: Duration = 15.seconds) async throws LocationError -> Position
    /// A stream of every position [`move(to:)`] sets, starting with the
    /// current one; it ends when the permission stops being `.granted`.
    func watch(accuracy: LocationAccuracy = .coarse) throws LocationError -> Stream<Position>

    /// Moves the device: [`currentPosition`] answers `position`, and every
    /// open [`watch`] yields it.
    func move(to position: Position)
    /// How the user answers the next prompts; `.granted` by default.
    func answerPrompts(with answer: PermissionState)
    /// Changes [`permission`] as the site's settings would; anything but
    /// `.granted` ends every open [`watch`].
    func setPermission(_ permission: PermissionState)
    /// Makes the next [`currentPosition`] throw `error`.
    func failNext(error: LocationError)
    /// Sets [`isSupported`]; while it is `false` every call throws
    /// `.unsupported`.
    func setSupported(_ value: Bool)
}

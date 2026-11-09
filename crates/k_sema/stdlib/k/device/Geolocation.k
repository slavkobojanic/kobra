/// The device's position (`docs/design/45-web-platform.md`,
/// "Geolocation"). A client-only platform interface:
/// [`BrowserGeolocation`] is injected with no `use` rule, and
/// [`k.test.TestGeolocation`] replaces it in tests.
///
/// Only [`requestAccess()`] asks the user, and only from a user's action
/// (a click handler, before its first `await`). Every other member checks
/// the permission first and never prompts: while it is `.prompt` they
/// throw [`LocationError.notAsked`].
@Platform(side: .client)
public interface Geolocation {
    /// Whether this browser can locate the device here: `false` outside a
    /// secure context (`http:` on a host other than `localhost`), and
    /// during server rendering. It doesn't change while the page is open.
    var isSupported: Bool

    /// Whether the app may read the position. Reactive: a view or a
    /// `@Derived` value that reads it updates when the user changes the
    /// site's settings. `.prompt` during server rendering.
    var permission: PermissionState

    /// Asks the user for access, unless [`permission`] already says
    /// `.granted` or `.denied`.
    ///
    /// - Returns: the new permission. A refusal is an answer, not an error.
    @UserAction
    func requestAccess() async -> PermissionState

    /// Reads the device's position once.
    ///
    /// - Parameters:
    ///   - accuracy: `.coarse` (a city, fast and light on the battery) or
    ///     `.precise`.
    ///   - maximumAge: accepts a cached position this old; `0.seconds`
    ///     asks for a fresh one.
    ///   - timeout: how long to wait for a fix.
    /// - Throws: [`LocationError.unsupported`] where [`isSupported`] is
    ///   `false`, `.notAsked` before [`requestAccess()`], `.denied` when
    ///   the user refused, `.unavailable` when the device has no fix and
    ///   `.timeout` when none came in time.
    func currentPosition(accuracy: LocationAccuracy = .coarse, maximumAge: Duration = 0.seconds, timeout: Duration = 15.seconds) async throws LocationError -> Position

    /// Follows the device's position: the stream yields the first position
    /// as soon as there is a fix, then each change. It ends when the
    /// permission is revoked ([`permission`] then says why) and when its
    /// owner is cancelled, which stops the browser's watch.
    ///
    /// - Throws: [`LocationError.unsupported`] where [`isSupported`] is
    ///   `false`, `.notAsked` or `.denied` when [`permission`] isn't
    ///   `.granted`.
    func watch(accuracy: LocationAccuracy = .coarse) throws LocationError -> Stream<Position>
}

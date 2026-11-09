/// The standard [`Geolocation`]: the browser's Geolocation API
/// (`navigator.geolocation`), with the permission read from the
/// Permissions API.
@ClientOnly
@Native
public service BrowserGeolocation implements Geolocation {
    /// `true` in a secure context whose browser can locate; see
    /// [`Geolocation.isSupported`].
    var isSupported: Bool
    /// The site's geolocation permission; see [`Geolocation.permission`].
    var permission: PermissionState

    /// Asks the browser for one position, which shows its prompt; see
    /// [`Geolocation.requestAccess()`].
    @UserAction
    func requestAccess() async -> PermissionState
    /// Reads one position; see [`Geolocation.currentPosition`].
    func currentPosition(accuracy: LocationAccuracy = .coarse, maximumAge: Duration = 0.seconds, timeout: Duration = 15.seconds) async throws LocationError -> Position
    /// Watches the position; see [`Geolocation.watch`].
    func watch(accuracy: LocationAccuracy = .coarse) throws LocationError -> Stream<Position>
}

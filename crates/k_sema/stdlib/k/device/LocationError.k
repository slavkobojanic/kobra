/// Why [`Geolocation`] has no position
/// (`docs/design/45-web-platform.md`, "Geolocation").
public enum LocationError implements Error, Equatable {
    /// This browser or context can't locate the device.
    case unsupported
    /// The user hasn't been asked yet: call
    /// [`Geolocation.requestAccess()`] from a user's action first.
    case notAsked
    /// The user, a browser setting or a policy refused.
    case denied
    /// The device has no fix.
    case unavailable
    /// No position came within the timeout.
    case timeout
}

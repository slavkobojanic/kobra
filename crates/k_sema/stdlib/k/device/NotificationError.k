/// Why [`Notifications.show(_:)`] showed nothing
/// (`docs/design/45-web-platform.md`, "Notifications").
public enum NotificationError implements Error, Equatable {
    /// This browser or context can't show notifications.
    case unsupported
    /// The user hasn't been asked yet: call
    /// [`Notifications.requestAccess()`] from a user's action first.
    case notAsked
    /// The user, a browser setting or a policy refused.
    case denied
}

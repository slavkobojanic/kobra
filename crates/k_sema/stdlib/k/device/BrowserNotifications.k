/// The standard [`Notifications`]: shown through the app's service worker
/// registration's `showNotification`, since Chromium on Android has no
/// `new Notification()`. `k build` emits the worker as `/k-sw.js` for an
/// app whose client reaches this service. The permission comes from the
/// Notifications API.
@ClientOnly
@Native
public service BrowserNotifications implements Notifications {
    /// `true` in a secure context with service workers and the
    /// Notifications API; see [`Notifications.isSupported`].
    var isSupported: Bool
    /// The site's notification permission; see
    /// [`Notifications.permission`].
    var permission: PermissionState

    /// Shows the browser's prompt; see [`Notifications.requestAccess()`].
    @UserAction
    func requestAccess() async -> PermissionState
    /// Shows a notification through the service worker; see
    /// [`Notifications.show(_:)`].
    func show(_ notification: SystemNotification) async throws NotificationError
}

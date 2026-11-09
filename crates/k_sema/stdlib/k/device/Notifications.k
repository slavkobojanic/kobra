/// Notifications the operating system shows
/// (`docs/design/45-web-platform.md`, "Notifications"): a timer finished,
/// a long export is ready, a message arrived on an open stream. A
/// client-only platform interface: [`BrowserNotifications`] is injected
/// with no `use` rule, and [`k.test.TestNotifications`] replaces it in
/// tests.
///
/// Only [`requestAccess()`] asks the user, and only from a user's action
/// (a click handler, before its first `await`). [`show(_:)`] checks the
/// permission first and never prompts: while it is `.prompt` it throws
/// [`NotificationError.notAsked`].
///
/// A click on a notification focuses the app's tab and opens its
/// [`SystemNotification.opens`] page, or opens a new tab at that page if
/// the app's tab was closed. There is no click callback: the notification
/// can outlive the page that showed it.
@Platform(side: .client)
public interface Notifications {
    /// Whether this browser can show notifications here: `false` outside a
    /// secure context, in a Safari tab on iOS (only an app added to the
    /// home screen may notify there), and during server rendering. It
    /// doesn't change while the page is open.
    var isSupported: Bool

    /// Whether the app may show notifications. Reactive: a view or a
    /// `@Derived` value that reads it updates when the user changes the
    /// site's settings. `.prompt` during server rendering.
    var permission: PermissionState

    /// Asks the user for permission, unless [`permission`] already says
    /// `.granted` or `.denied`.
    ///
    /// - Returns: the new permission: `.prompt` again when the user
    ///   dismissed the prompt without answering. A refusal is an answer,
    ///   not an error.
    @UserAction
    func requestAccess() async -> PermissionState

    /// Shows `notification`. A later notification with the same `tag`
    /// replaces it.
    ///
    /// - Throws: [`NotificationError.unsupported`] where [`isSupported`] is
    ///   `false`, `.notAsked` before [`requestAccess()`] and `.denied` when
    ///   the user refused.
    func show(_ notification: SystemNotification) async throws NotificationError
}

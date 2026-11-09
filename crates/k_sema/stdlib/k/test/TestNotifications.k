import k.device.NotificationError
import k.device.Notifications
import k.device.PermissionState
import k.device.SystemNotification

/// The default [`Notifications`] in `k test`
/// (`docs/design/45-web-platform.md`, "Notifications"): no prompt opens
/// and nothing reaches the operating system. The browser is capable, the
/// permission starts at `.prompt`, and the user answers prompts with
/// `.granted`.
@TestDefault(replaces: Notifications)
@Native
public service TestNotifications implements Notifications {
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// `.prompt` until a prompt is answered or [`setPermission(_:)`]
    /// changes it. Reactive, like the browser's.
    var permission: PermissionState
    /// Every notification shown, oldest first, including those a later one
    /// with the same tag replaced.
    var shown: List<SystemNotification>
    /// The number of [`requestAccess()`] calls that asked the user.
    var prompts: Int

    /// Asks while [`permission`] is `.prompt`, counting it in [`prompts`]
    /// and answering what [`answerPrompts(with:)`] set; otherwise answers
    /// the permission as it is.
    func requestAccess() async -> PermissionState
    /// Records `notification` in [`shown`]; throws `.unsupported`,
    /// `.notAsked` or `.denied` as the browser would.
    func show(_ notification: SystemNotification) async throws NotificationError

    /// How the user answers the next prompts; `.granted` by default.
    func answerPrompts(with answer: PermissionState)
    /// Changes [`permission`] as the site's settings would.
    func setPermission(_ permission: PermissionState)
    /// Sets [`isSupported`]; while it is `false`, [`show(_:)`] throws
    /// `.unsupported` and [`requestAccess()`] asks nothing.
    func setSupported(_ value: Bool)
    /// Plays the user clicking `notification`, which must have been shown:
    /// the app's tab opens its `opens` page (`screen.currentPage` says
    /// which), or stays where it is when `opens` is `nil`.
    func click(_ notification: SystemNotification)
}

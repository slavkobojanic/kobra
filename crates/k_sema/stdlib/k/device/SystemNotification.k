/// A notification the operating system shows
/// (`docs/design/45-web-platform.md`, "Notifications"). `Codable`, so a
/// push message (Tier 2) can carry the same value.
public struct SystemNotification implements Equatable, Codable {
    /// The first line, in bold on most systems.
    let title: String
    /// The text under the title.
    let body: String? = nil
    /// A later notification with the same tag replaces this one, rather
    /// than adding another.
    let tag: String? = nil
    /// The page a click opens; `nil` focuses the app's tab where it is (or
    /// opens the page that showed the notification, if the tab was
    /// closed).
    let opens: Page? = nil
}

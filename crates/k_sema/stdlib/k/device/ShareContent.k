/// What to share (`docs/design/45-web-platform.md`, "Web Share"): a page
/// of the app, a link or text, with an optional title. Built by
/// [`page(_:title:)`], [`link(_:title:)`] or [`text(_:title:)`], so it
/// always carries something to share.
public struct ShareContent implements Equatable {
    /// The link, absolute.
    let url: Url?
    /// The text.
    let text: String?
    /// The title, which some share targets show.
    let title: String?

    /// Private, so content is built only by the factories below and is
    /// never empty.
    @Native
    private init() {}

    /// A page of the app, made absolute against the page's origin in the
    /// browser (during server rendering its URL is the page's path).
    @Native
    static func page(_ page: Page, title: String? = nil) -> ShareContent
    /// A link.
    @Native
    static func link(_ url: Url, title: String? = nil) -> ShareContent
    /// Text.
    @Native
    static func text(_ text: String, title: String? = nil) -> ShareContent
}

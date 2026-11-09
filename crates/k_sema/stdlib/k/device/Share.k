/// The system share sheet (`docs/design/45-web-platform.md`, "Web
/// Share"). A client-only platform interface: [`BrowserShare`] is
/// injected with no `use` rule, and [`k.test.TestShare`] replaces it in
/// tests. For the common "share this" button a view needs no service:
/// [`ShareButton`] does it.
@Platform(side: .client)
public interface Share {
    /// Whether this browser has a share sheet here: `false` on most desktop
    /// browsers without one, outside a secure context and during server
    /// rendering. It doesn't change while the page is open.
    var isSupported: Bool

    /// Opens the share sheet with `content` and returns when the user has
    /// picked a target. It uses up the user's action, so one click shares
    /// once.
    ///
    /// - Throws: [`ShareError.unsupported`] where [`isSupported`] is
    ///   `false` or the browser can't share `content`,
    ///   [`ShareError.cancelled`] when the user closes the sheet, and
    ///   [`ShareError.denied`] when a policy refuses.
    @UserAction(consuming: true)
    func share(_ content: ShareContent) async throws ShareError
}

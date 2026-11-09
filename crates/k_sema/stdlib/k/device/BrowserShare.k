/// The standard [`Share`]: the browser's Web Share API
/// (`navigator.share`).
@ClientOnly
@Native
public service BrowserShare implements Share {
    /// `true` in a secure context whose browser can share; see
    /// [`Share.isSupported`].
    var isSupported: Bool

    /// Opens the browser's share sheet; see [`Share.share(_:)`].
    @UserAction(consuming: true)
    func share(_ content: ShareContent) async throws ShareError
}

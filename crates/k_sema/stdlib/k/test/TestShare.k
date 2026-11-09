import k.device.Share
import k.device.ShareContent
import k.device.ShareError

/// The default [`Share`] in `k test` (`docs/design/45-web-platform.md`,
/// "Web Share"): no sheet opens; every share is recorded, and the user
/// picks a target unless the test says otherwise.
@TestDefault(replaces: Share)
@Native
public service TestShare implements Share {
    /// Everything shared so far, oldest first.
    var shared: List<ShareContent>
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool

    /// Records `content` in [`shared`], or throws what [`cancelNext()`] or
    /// [`failNext(error:)`] set (`.unsupported` after
    /// `setSupported(false)`).
    func share(_ content: ShareContent) async throws ShareError

    /// Makes the user close the next share sheet:
    /// [`ShareError.cancelled`].
    func cancelNext()
    /// Makes the next share throw `error`.
    func failNext(error: ShareError)
    /// Sets [`isSupported`]; while it is `false` every share throws
    /// `.unsupported`, and a [`k.device.ShareButton`] copies instead.
    func setSupported(_ value: Bool)
}

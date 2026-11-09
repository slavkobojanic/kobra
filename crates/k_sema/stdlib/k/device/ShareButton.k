/// A button that shares `content` (`docs/design/45-web-platform.md`,
/// "ShareButton"): `ShareButton(content: .page(.recipe(id: recipe.id),
/// title: recipe.title), label: "Share recipe")`.
///
/// Where the browser can share it opens the share sheet; where it can't,
/// it copies the link (or the text) to the clipboard and says "Link
/// copied" (or "Copied") for two seconds in a polite live region. It
/// renders the same button either way, so server rendering and hydration
/// agree. It uses the injected [`Share`] and [`Clipboard`], so in `k test`
/// it shares to [`k.test.TestShare`] and copies to
/// [`k.test.TestClipboard`].
@Native
public component ShareButton {
    /// What to share, read when the button is clicked.
    @Prop let content: ShareContent
    /// The button's name and visible text ("Share recipe").
    @Prop let label: String

    /// Fires after the user picked a share target.
    @Emits let shared: Event<Void>
    /// Fires after the link or text was copied instead.
    @Emits let copied: Event<Void>
    /// Fires when it could neither share nor copy. Closing the sheet fires
    /// nothing. When nothing listens, the button shows "Couldn't share".
    @Emits let failed: Event<ShareButtonFailure>
}

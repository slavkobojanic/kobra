/// A button that copies `text` to the clipboard
/// (`docs/design/45-web-platform.md`, "CopyButton"):
/// `CopyButton(text: invite.url, label: "Copy invite link")`.
///
/// Its accessible name is `label`. After a copy it shows "Copied" for two
/// seconds (on the injected [`k.time.Clock`]) and announces it in a polite
/// live region; the button keeps its size. It uses the injected
/// [`Clipboard`], so in `k test` it writes to [`k.test.TestClipboard`].
/// It renders on the server as it will in the browser, and copies once
/// the page has hydrated.
@Native
public component CopyButton {
    /// What to copy, read when the button is clicked.
    @Prop let text: String
    /// The button's name and visible text ("Copy invite link").
    @Prop let label: String

    /// Fires after a successful copy.
    @Emits let copied: Event<Void>
    /// Fires when the copy failed. When nothing listens, the button shows
    /// "Couldn't copy" instead of "Copied", so the user knows to select the
    /// text.
    @Emits let failed: Event<ClipboardWriteError>
}

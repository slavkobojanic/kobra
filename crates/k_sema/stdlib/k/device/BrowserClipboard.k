/// The standard [`Clipboard`]: the browser's asynchronous clipboard
/// (`navigator.clipboard`).
@ClientOnly
@Native
public service BrowserClipboard implements Clipboard {
    /// `true` in a secure context whose browser has the clipboard API; see
    /// [`Clipboard.isSupported`].
    var isSupported: Bool

    /// Writes `text` with the browser's clipboard; see
    /// [`Clipboard.write(text:)`].
    @UserAction
    func write(text: String) async throws ClipboardWriteError
    /// Reads the browser's clipboard as text; see
    /// [`Clipboard.readText()`].
    @UserAction
    func readText() async throws ClipboardReadError -> String?
}

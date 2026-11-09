/// Plain text on the system clipboard (`docs/design/45-web-platform.md`,
/// "Clipboard"). A client-only platform interface: [`BrowserClipboard`] is
/// injected with no `use` rule, and [`k.test.TestClipboard`] replaces it in
/// tests. For the common "copy this" button a view needs no service:
/// [`CopyButton`] does it.
///
/// Both members must be called from a user's action (a click handler)
/// before its first `await`: browsers allow clipboard access only then.
@Platform(side: .client)
public interface Clipboard {
    /// Whether this browser can use the clipboard here: `false` outside a
    /// secure context (`http:` on a host other than `localhost`), and
    /// during server rendering. It doesn't change while the page is open.
    var isSupported: Bool

    /// Replaces the clipboard's content with `text`.
    ///
    /// - Throws: [`ClipboardWriteError.unsupported`] where
    ///   [`isSupported`] is `false`, [`ClipboardWriteError.denied`] when
    ///   the browser refuses.
    @UserAction
    func write(text: String) async throws ClipboardWriteError

    /// Reads the clipboard's text. Some browsers show a "Paste"
    /// confirmation for each read.
    ///
    /// - Returns: the text, or `nil` when the clipboard holds no text (an
    ///   image, or nothing).
    /// - Throws: [`ClipboardReadError.unsupported`] where [`isSupported`]
    ///   is `false`, [`ClipboardReadError.denied`] when the browser or the
    ///   user refuses.
    @UserAction
    func readText() async throws ClipboardReadError -> String?
}

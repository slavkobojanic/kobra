import k.device.Clipboard
import k.device.ClipboardReadError
import k.device.ClipboardWriteError

/// The default [`Clipboard`] in `k test` (`docs/design/45-web-platform.md`,
/// "Clipboard"): a clipboard in memory, empty for every test, never the
/// machine's.
@TestDefault(replaces: Clipboard)
@Native
public service TestClipboard implements Clipboard {
    /// What the clipboard holds; `nil` until something is copied.
    var text: String?
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// The number of writes and reads so far, failed ones included.
    var calls: Int

    /// Puts `text` on the clipboard, or throws the error set by
    /// [`failWrites(error:)`] (`.unsupported` after `setSupported(false)`).
    func write(text: String) async throws ClipboardWriteError
    /// Answers [`text`], or throws the error set by [`failReads(error:)`]
    /// (`.unsupported` after `setSupported(false)`).
    func readText() async throws ClipboardReadError -> String?

    /// Puts `text` on the clipboard, as if another application copied it.
    func set(text: String)
    /// Makes every following write throw `error`.
    func failWrites(error: ClipboardWriteError)
    /// Makes every following read throw `error`.
    func failReads(error: ClipboardReadError)
    /// Sets [`isSupported`]; while it is `false` every call throws
    /// `.unsupported`.
    func setSupported(_ value: Bool)
}

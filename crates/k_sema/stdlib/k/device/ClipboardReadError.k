/// Why [`Clipboard.readText()`] read nothing
/// (`docs/design/45-web-platform.md`, "Clipboard").
public enum ClipboardReadError implements Error, Equatable {
    /// This browser or context has no clipboard ([`Clipboard.isSupported`]
    /// is `false`).
    case unsupported
    /// The browser or the user refused, which may happen at each read.
    case denied
}
